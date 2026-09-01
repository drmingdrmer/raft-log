use std::io;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use chunked_wal::ChunkPersistedFn;
use chunked_wal::ChunkedWal;
use log::info;

use crate::ChunkId;
use crate::Config;
use crate::RaftLogRecord;
use crate::RaftWalTypes;
use crate::Types;
use crate::WALRecord;
use crate::api::raft_log_writer::RaftLogWriter;
use crate::api::state_machine::StateMachine;
use crate::api::wal::WAL;
use crate::errors::LogIndexNotFound;
use crate::errors::RaftLogStateError;
use crate::raft_log::access_state::AccessStat;
use crate::raft_log::dump::RefDump;
use crate::raft_log::dump_raft_log::DumpRaftLog;
use crate::raft_log::raft_log_action::RaftLogAction;
use crate::raft_log::stat::Stat;
use crate::raft_log::state_machine::RaftLogStateMachine;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;
use crate::types::Segment;

/// RaftLog is a Write-Ahead-Log implementation for the Raft consensus protocol.
///
/// It provides persistent storage for Raft log entries and state, with the
/// following features:
/// - Append-only log storage with chunk-based organization
/// - In-memory caching of log payloads
/// - Exclusive file locking for thread-safe operations
/// - Support for log truncation and purging
/// - Statistics tracking for monitoring
#[derive(Debug)]
pub struct RaftLog<T: Types> {
    pub(crate) config: Arc<Config>,

    pub(crate) wal: ChunkedWal<RaftWalTypes<T>>,

    pub(crate) state_machine: RaftLogStateMachine<T>,

    /// The chunk IDs whose logs are all purged, waiting to be deleted.
    ///
    /// Deleting a chunk file is only safe once the purge record covering it is
    /// fsynced, so [`RaftLogWriter::flush`] with `sync` is what hands these
    /// IDs to the FlushWorker. This buffer is process memory and does not
    /// survive a restart; [`RaftLog::open`] rebuilds it from the replayed
    /// `purged` state.
    removed_chunks: Vec<ChunkId>,

    access_stat: AccessStat,
}

impl<T: Types> RaftLogWriter<T> for RaftLog<T> {
    fn save_user_data(
        &mut self,
        user_data: Option<T::UserData>,
    ) -> Result<Segment, io::Error> {
        let mut state = self.state_machine.checkpoint();
        state.user_data = user_data;
        let record = RaftLogRecord::Checkpoint(state);
        self.append_and_apply(&record)
    }

    fn save_vote(&mut self, vote: T::Vote) -> Result<Segment, io::Error> {
        let record = RaftLogRecord::Action(RaftLogAction::SaveVote(vote));
        self.append_and_apply(&record)
    }

    fn append<I>(&mut self, entries: I) -> Result<Segment, io::Error>
    where I: IntoIterator<Item = (T::LogId, T::LogPayload)> {
        for (log_id, payload) in entries {
            let record =
                RaftLogRecord::Action(RaftLogAction::Append(log_id, payload));
            self.append_and_apply(&record)?;
        }
        Ok(self.wal.last_segment())
    }

    /// Truncate at `index`, keep the record before `index`.
    fn truncate(&mut self, index: u64) -> Result<Segment, io::Error> {
        let purged = self.log_state().purged.as_ref();

        let log_id = if index == T::next_log_index(purged) {
            purged.cloned()
        } else {
            // Reaching here with `index == 0` means the log starts above index
            // 0, so the entry this call would keep is already purged. Without
            // the guard `index - 1` underflows: it panics in a debug build and
            // wraps to `u64::MAX` in a release build.
            let Some(prev_index) = index.checked_sub(1) else {
                let err = LogIndexNotFound::new(index);
                return Err(RaftLogStateError::<T>::from(err).into());
            };

            let log_id = self.get_log_id(prev_index)?;
            Some(log_id)
        };

        let record =
            RaftLogRecord::Action(RaftLogAction::TruncateAfter(log_id));
        self.append_and_apply(&record)
    }

    fn purge(&mut self, upto: T::LogId) -> Result<Segment, io::Error> {
        let purged = self.log_state().purged.as_ref();

        info!(
            "RaftLog purge upto: {:?}; current purged: {:?}",
            upto, purged
        );

        if T::log_index(&upto) < T::next_log_index(purged) {
            return Ok(self.wal.last_segment());
        }

        let record =
            RaftLogRecord::Action(RaftLogAction::PurgeUpto(upto.clone()));
        let res = self.append_and_apply(&record)?;

        self.buffer_removable_chunks();

        Ok(res)
    }

    fn commit(&mut self, log_id: T::LogId) -> Result<Segment, io::Error> {
        let record = RaftLogRecord::Action(RaftLogAction::Commit(log_id));
        self.append_and_apply(&record)
    }

    fn flush(
        &mut self,
        sync: bool,
        callback: Option<T::Callback>,
    ) -> Result<(), io::Error> {
        self.wal.send_pending(sync, callback)?;

        // The FlushWorker drains its queue in order, so queuing the removal
        // behind the sync above is what guarantees the covering purge record
        // reaches the disk before any chunk file is unlinked. Without a sync
        // there is nothing to order against, so the removal keeps waiting.
        if sync && !self.removed_chunks.is_empty() {
            let chunk_ids = self.removed_chunks.drain(..).collect::<Vec<_>>();
            self.wal.send_remove_chunks(chunk_ids)?;
        }

        Ok(())
    }
}

impl<T: Types> RaftLog<T> {
    /// Dump the RaftLog data for debugging purposes.
    ///
    /// Returns a `DumpRaftLog` struct containing a complete snapshot of the
    /// RaftLog state.
    pub fn dump_data(&self) -> DumpRaftLog<T> {
        let logs = self.state_machine.log.values().cloned().collect::<Vec<_>>();
        let cache =
            self.state_machine.payload_cache.read().unwrap().cache.clone();
        let record_reader = self.wal.closed_chunk_reader();

        DumpRaftLog {
            state: self.state_machine.checkpoint(),
            logs,
            cache,
            record_reader,
            cache_hit: 0,
            cache_miss: 0,
        }
    }

    /// Dump the WAL data in this Raft-log for debugging purposes.
    ///
    /// This method returns a reference type `RefDump` containing the RaftLog
    /// configuration and the RaftLog instance itself.
    pub fn dump(&self) -> RefDump<'_, T> {
        RefDump { raft_log: self }
    }

    /// Get a reference to the RaftLog configuration.
    pub fn config(&self) -> &Config {
        self.config.as_ref()
    }

    /// Opens a RaftLog at the specified directory.
    ///
    /// This operation:
    /// 1. Acquires an exclusive lock on the directory
    /// 2. Loads existing chunks in order
    /// 3. Replays WAL records to rebuild the state
    /// 4. Creates a new open chunk for future writes
    ///
    /// # Errors
    /// Returns an error if:
    /// - Directory operations fail
    /// - There are gaps between chunk offsets
    /// - WAL records are invalid
    pub fn open(config: Arc<Config>) -> Result<Self, io::Error> {
        let wal_config = Arc::new(config.wal.clone());

        let mut sm = RaftLogStateMachine::new(&config);
        let cache = sm.payload_cache.clone();
        let on_chunk_persisted: ChunkPersistedFn<RaftWalTypes<T>> =
            Arc::new(move |_persisted, prev_chunk_checkpoint: Option<Arc<RaftLogState<T>>>| {
                let Some(prev_chunk_checkpoint) = prev_chunk_checkpoint else {
                    return;
                };

                cache
                    .write()
                    .unwrap()
                    .set_last_evictable(prev_chunk_checkpoint.last().cloned());
            });

        let wal = ChunkedWal::open(wal_config, &mut sm, on_chunk_persisted)?;

        let mut s = Self {
            config,
            state_machine: sm,
            wal,
            access_stat: Default::default(),
            removed_chunks: vec![],
        };

        // `removed_chunks` starts empty, so a purge from an earlier run whose
        // `RemoveChunks` request never ran left its chunks on disk. The
        // replayed `purged` state is enough to find them again.
        s.buffer_removable_chunks();

        Ok(s)
    }

    /// Replace the RaftLog state and append the new state to the WAL as a
    /// checkpoint.
    ///
    /// `state` must describe the log entries this store holds, which are
    /// exactly the entries in `(purged, last]`. A store whose log is empty
    /// accepts any state, which is what restoring from a snapshot needs. Any
    /// other state is rejected, because storing it would leave [`Self::read`]
    /// serving entries the state calls purged, or hiding entries the state
    /// calls present.
    ///
    /// So `vote`, `committed` and `user_data` are free to change here, while
    /// `last` and `purged` are not once the log holds an entry. To change only
    /// `user_data`, call [`RaftLogWriter::save_user_data`], which carries the
    /// log fields over unchanged.
    pub fn update_state(
        &mut self,
        state: RaftLogState<T>,
    ) -> Result<Segment, io::Error> {
        // Only a live store can be checked this way, so `append_and_apply`
        // does not run this check and this is its sole call site.
        // `RaftLogStateMachine::check_checkpoint` says why replay cannot.
        self.state_machine.check_checkpoint(&state)?;

        let record = RaftLogRecord::Checkpoint(state);
        self.append_and_apply(&record)
    }

    /// Reads log entries in the specified index range.
    ///
    /// Returns an iterator over log entries, attempting to serve them from
    /// cache first, falling back to disk reads if necessary.
    pub fn read(
        &self,
        from: u64,
        to: u64,
    ) -> impl Iterator<Item = Result<(T::LogId, T::LogPayload), io::Error>> + '_
    {
        self.state_machine.log.range(from..to).map(|(_, log_data)| {
            let log_id = log_data.log_id.clone();

            let payload =
                self.state_machine.payload_cache.read().unwrap().get(&log_id);

            let payload = if let Some(payload) = payload {
                self.access_stat.cache_hit.fetch_add(1, Ordering::Relaxed);
                payload
            } else {
                self.access_stat.cache_miss.fetch_add(1, Ordering::Relaxed);

                let record = self
                    .wal
                    .load_record(&log_data.chunk_id, log_data.record_segment)?;

                if let WALRecord::Action(RaftLogAction::Append(
                    log_id,
                    payload,
                )) = record
                {
                    debug_assert_eq!(log_id, log_data.log_id);
                    payload
                } else {
                    panic!("Expect Record::Append but: {:?}", record);
                }
            };

            Ok((log_id, payload))
        })
    }

    /// Get a reference to the latest RaftLog state.
    ///
    /// The state is the latest state of the RaftLog, even if the corresponding
    /// WAL record is not committed yet.
    pub fn log_state(&self) -> &RaftLogState<T> {
        &self.state_machine.log_state
    }

    #[allow(dead_code)]
    pub(crate) fn log_state_mut(&mut self) -> &mut RaftLogState<T> {
        &mut self.state_machine.log_state
    }

    /// Get a reference to the RaftLog statistics.
    ///
    /// This method returns a `Stat` struct containing statistics about the
    /// RaftLog, including:
    /// - The number of closed chunks
    /// - The open chunk statistics
    pub fn stat(&self) -> Stat<T> {
        let closed = self.wal.closed_chunk_stats();
        let open_stat =
            self.wal.open_chunk_stat(self.state_machine.checkpoint());
        let cache = self.state_machine.payload_cache.read().unwrap();

        Stat {
            closed_chunks: closed,
            open_chunk: open_stat,

            payload_cache_last_evictable: cache.last_evictable().cloned(),
            payload_cache_item_count: cache.item_count() as u64,
            payload_cache_max_item: cache.max_items() as u64,
            payload_cache_size: cache.total_size() as u64,
            payload_cache_capacity: cache.capacity() as u64,

            payload_cache_miss: self
                .access_stat
                .cache_miss
                .load(Ordering::Relaxed),
            payload_cache_hit: self
                .access_stat
                .cache_hit
                .load(Ordering::Relaxed),

            flush_metrics: self.wal.flush_metrics(),
        }
    }

    /// Get a reference to the access statistics.
    ///
    /// This method returns a reference to the `AccessStat` struct, which
    /// contains statistics about the access patterns of the RaftLog.
    pub fn access_stat(&self) -> &AccessStat {
        &self.access_stat
    }

    /// Block until the FlushWorker has processed all queued requests.
    ///
    /// After this returns, all file writes, syncs, and cache eviction boundary
    /// updates issued before this call are complete. Note that the payload
    /// cache item count may still be non-deterministic because eviction is
    /// lazy; call `drain_cache_evictable()` afterwards to normalize it.
    pub fn wait_worker_idle(&self) -> Result<(), io::Error> {
        self.wal.wait_worker_idle()
    }

    /// Drain all evictable entries from the payload cache.
    ///
    /// Call after `wait_worker_idle()` to normalize the cache to a
    /// deterministic state. See `PayloadCache::drain_evictable` for details.
    pub fn drain_cache_evictable(&self) {
        self.state_machine.payload_cache.write().unwrap().drain_evictable();
    }

    fn get_log_id(&self, index: u64) -> Result<T::LogId, RaftLogStateError<T>> {
        let entry = self
            .state_machine
            .log
            .get(&index)
            .ok_or_else(|| LogIndexNotFound::new(index))?;
        Ok(entry.log_id.clone())
    }

    /// Move the closed chunks that hold only purged logs into
    /// [`Self::removed_chunks`].
    ///
    /// A closed chunk is removable when its trailing checkpoint has
    /// `last <= purged`: every log id it holds is at or below the purge point,
    /// so no entry in the log map points into it. The scan stops at the first
    /// chunk that fails the test, which keeps the drain to the oldest run of
    /// removable chunks.
    ///
    /// Two events can make a chunk removable, and both call this: a chunk
    /// closing at or below the purge point, and a `purge` that advances
    /// `purged`. [`RaftLog::open`] calls it too, to rebuild the buffer that a
    /// restart threw away.
    ///
    /// The boundary is the current `purged` state, not the `upto` argument of
    /// a `purge` call in progress, so the result is the same whichever event
    /// triggered the call.
    fn buffer_removable_chunks(&mut self) {
        let Some(purged) = self.log_state().purged.clone() else {
            return;
        };

        let chunk_ids = self.wal.drain_closed_chunks_while(|state| {
            state.last.as_ref() <= Some(&purged)
        });

        for chunk_id in chunk_ids {
            info!(
                "RaftLog: scheduled to remove chunk after next flush: {}",
                self.config.wal.chunk_path(chunk_id)
            );
            self.removed_chunks.push(chunk_id);
        }
    }

    fn append_and_apply(
        &mut self,
        rec: &RaftLogRecord<T>,
    ) -> Result<Segment, io::Error> {
        // Reject the record before it reaches the WAL buffer, so that a
        // rejected write changes nothing: not the log map, not the payload
        // cache, not the state, and not what the next flush persists. See
        // `RaftLogStateMachine::check`.
        self.state_machine.check(rec)?;

        WAL::append(&mut self.wal, rec)?;
        StateMachine::apply(
            &mut self.state_machine,
            rec,
            self.wal.open_chunk_id(),
            self.wal.last_segment(),
        )?;

        let closed = self.wal.try_close_full_chunk(&self.state_machine)?;

        // A chunk that closes at or below the purge point is obsolete right
        // away. Example: the log is fully purged at `(1,1)`, so `last` and
        // `purged` are both `(1,1)`; `save_vote` carries no log entry, so every
        // chunk it closes still checkpoints `last == (1,1)` and holds nothing
        // live. Buffering on the close event retires those chunks at the next
        // sync flush, instead of waiting for a later `purge` call that a node
        // with an empty log may never make.
        if closed.is_some() {
            self.buffer_removable_chunks();
        }

        Ok(self.wal.last_segment())
    }

    /// Returns the current size of the log on disk in bytes.
    ///
    /// This includes all closed chunks and the open chunk, measuring from the
    /// start of the earliest chunk to the end of the open chunk.
    pub fn on_disk_size(&self) -> u64 {
        self.wal.on_disk_size()
    }
}
