use std::collections::BTreeMap;
use std::io;
use std::io::Error;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use codeq::OffsetSize;
use codeq::Segment;

use crate::api::raft_log_writer::RaftLogWriter;
use crate::api::state_machine::StateMachine;
use crate::api::wal::WAL;
use crate::chunk::closed_chunk::ClosedChunk;
use crate::chunk::open_chunk::OpenChunk;
use crate::chunk::Chunk;
use crate::errors::LogIndexNotFound;
use crate::errors::RaftLogStateError;
use crate::file_lock;
use crate::num::format_pad_u64;
use crate::raft_log::access_state::AccessStat;
use crate::raft_log::dump::RefDump;
use crate::raft_log::dump_data::DumpData;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;
use crate::raft_log::state_machine::RaftLogStateMachine;
use crate::raft_log::wal::RaftLogWAL;
use crate::ChunkId;
use crate::Config;
use crate::Types;
use crate::WALRecord;

#[derive(Debug)]
pub struct RaftLog<T: Types> {
    pub(crate) config: Arc<Config>,

    /// Acquire the dir exclusive lock when writing to the log.
    _dir_lock: file_lock::FileLock,

    pub(crate) wal: RaftLogWAL<T>,

    pub(crate) state_machine: RaftLogStateMachine<T>,

    /// The chunk paths that are no longer needed because all logs in them are
    /// purged. But removing them must be postponed until the purge record
    /// is flushed to disk.
    removed_chunks: Vec<String>,

    state: AccessStat,
}

impl<T: Types> RaftLogWriter<T> for RaftLog<T> {
    fn save_user_data(
        &mut self,
        user_data: Option<T::UserData>,
    ) -> Result<Segment, Error> {
        let mut state = self.log_state().clone();
        state.user_data = user_data;
        let record = WALRecord::State(state);
        self.append_and_apply(&record)
    }

    fn save_vote(&mut self, vote: T::Vote) -> Result<Segment, io::Error> {
        let record = WALRecord::SaveVote(vote.clone());
        self.append_and_apply(&record)
    }

    fn append<I>(&mut self, entries: I) -> Result<Segment, io::Error>
    where I: IntoIterator<Item = (T::LogId, T::LogPayload)> {
        for (log_id, payload) in entries {
            let record = WALRecord::Append(log_id, payload);
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
            let log_id = self.get_log_id(index - 1)?;
            Some(log_id)
        };

        let record = WALRecord::TruncateAfter(log_id);
        self.append_and_apply(&record)
    }

    fn purge(&mut self, upto: T::LogId) -> Result<Segment, io::Error> {
        // NOTE that only when the purge record is committed, the chunk file can
        // be removed.

        let purged = self.log_state().purged.as_ref();

        if T::log_index(&upto) < T::next_log_index(purged) {
            return Ok(self.wal.last_segment());
        }

        let record = WALRecord::PurgeUpto(upto.clone());
        let res = self.append_and_apply(&record)?;

        // Buffer the chunk ids to remove.
        // After the purge record is flushed to disk,
        // remove them in the FlushWorker

        while let Some((_chunk_id, closed)) = self.wal.closed.first_key_value()
        {
            if closed.state.last.as_ref() > Some(&upto) {
                break;
            }
            let (chunk_id, _r) = self.wal.closed.pop_first().unwrap();
            let path = self.config.chunk_path(chunk_id);
            self.removed_chunks.push(path);
        }

        Ok(res)
    }

    fn commit(&mut self, log_id: T::LogId) -> Result<Segment, io::Error> {
        let record = WALRecord::Commit(log_id);
        self.append_and_apply(&record)
    }

    fn flush(&mut self, callback: T::Callback) -> Result<(), io::Error> {
        self.wal.send_flush(callback)?;

        if !self.removed_chunks.is_empty() {
            let chunk_ids = self.removed_chunks.drain(..).collect::<Vec<_>>();
            self.wal.send_remove_chunks(chunk_ids)?;
        }

        Ok(())
    }
}

impl<T: Types> RaftLog<T> {
    /// Dump log data
    pub fn dump_data(&self) -> DumpData<T> {
        let logs = self.state_machine.log.values().cloned().collect::<Vec<_>>();
        let cache =
            self.state_machine.payload_cache.read().unwrap().cache.clone();
        let chunks = self.wal.closed.clone();

        DumpData {
            state: self.state_machine.log_state.clone(),
            logs,
            cache,
            chunks,
            cache_hit: 0,
            cache_miss: 0,
        }
    }

    pub fn dump(&self) -> RefDump<'_, T> {
        RefDump {
            config: self.config.clone(),
            raft_log: self,
        }
    }

    pub fn config(&self) -> &Config {
        self.config.as_ref()
    }

    pub fn open(config: Arc<Config>) -> Result<Self, io::Error> {
        let dir_lock = file_lock::FileLock::new(config.clone())?;

        let offsets = Self::load_chunk_ids(&config)?;

        let mut sm = RaftLogStateMachine::new(&config);
        let mut closed = BTreeMap::new();
        let mut prev_end_offset = None;

        for chunk_id in offsets {
            if let Some(prev_end) = prev_end_offset {
                if prev_end != chunk_id.offset() {
                    let message = format!(
                        "Gap between chunks: {} -> {}; Can not open, \
                        fix this error and re-open",
                        format_pad_u64(prev_end),
                        format_pad_u64(chunk_id.offset()),
                    );
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        message,
                    ));
                }
            }

            let (chunk, records) = Chunk::open(config.clone(), chunk_id)?;

            prev_end_offset = Some(chunk.last_segment().end());

            for (i, record) in records.into_iter().enumerate() {
                let start = chunk.global_offsets[i];
                let end = chunk.global_offsets[i + 1];
                let seg = Segment::new(start, end - start);
                sm.apply(&record, chunk_id, seg)?;
            }

            // On disk logs can be evicted from the cache.
            let last_log_id = sm.log_state.last.clone();
            sm.payload_cache.write().unwrap().set_last_evictable(last_log_id);

            closed.insert(
                chunk_id,
                ClosedChunk::new(chunk, sm.log_state.clone()),
            );
        }

        let open = OpenChunk::create(
            config.clone(),
            ChunkId(prev_end_offset.unwrap_or_default()),
            WALRecord::State(sm.log_state.clone()),
        )?;

        let cache = sm.payload_cache.clone();

        let wal = RaftLogWAL::new(config.clone(), closed, open, cache);

        let s = Self {
            config,
            _dir_lock: dir_lock,
            state_machine: sm,
            wal,
            state: Default::default(),
            removed_chunks: vec![],
        };

        Ok(s)
    }

    pub fn load_chunk_ids(config: &Config) -> Result<Vec<ChunkId>, io::Error> {
        let path = &config.dir;
        let entries = std::fs::read_dir(path)?;
        let mut chunk_ids = vec![];
        for entry in entries {
            let entry = entry?;
            let file_name = entry.file_name();

            let fn_str = file_name.to_string_lossy();
            let res = Config::parse_chunk_file_name(&fn_str);

            match res {
                Ok(offset) => {
                    chunk_ids.push(ChunkId(offset));
                }
                Err(err) => {
                    log::warn!(
                        "Ignore invalid WAL file name: '{}': {}",
                        fn_str,
                        err
                    );
                    continue;
                }
            };
        }

        chunk_ids.sort();

        Ok(chunk_ids)
    }

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
                self.state.cache_hit.fetch_add(1, Ordering::Relaxed);
                payload
            } else {
                self.state.cache_miss.fetch_add(1, Ordering::Relaxed);
                self.wal.load_log_payload(log_data)?
            };

            Ok((log_id, payload))
        })
    }

    pub fn log_state(&self) -> &RaftLogState<T> {
        &self.state_machine.log_state
    }

    #[allow(dead_code)]
    pub(crate) fn log_state_mut(&mut self) -> &mut RaftLogState<T> {
        &mut self.state_machine.log_state
    }

    pub fn access_stat(&self) -> &AccessStat {
        &self.state
    }

    fn get_log_id(&self, index: u64) -> Result<T::LogId, RaftLogStateError<T>> {
        let entry = self
            .state_machine
            .log
            .get(&index)
            .ok_or_else(|| LogIndexNotFound::new(index))?;
        Ok(entry.log_id.clone())
    }

    fn append_and_apply(
        &mut self,
        rec: &WALRecord<T>,
    ) -> Result<Segment, io::Error> {
        WAL::append(&mut self.wal, rec)?;
        StateMachine::apply(
            &mut self.state_machine,
            rec,
            self.wal.open.chunk.chunk_id(),
            self.wal.last_segment(),
        )?;

        self.wal
            .try_close_full_chunk(|| self.state_machine.log_state.clone())?;

        Ok(self.wal.last_segment())
    }

    pub fn on_disk_size(&self) -> u64 {
        let end = self.wal.open.chunk.global_end();
        let open_start = self.wal.open.chunk.global_start();
        let first_closed_start  = self.wal.closed.first_key_value().map(|(_, v)| v.chunk.global_start()).unwrap_or(open_start);

        end- first_closed_start

    }
}
