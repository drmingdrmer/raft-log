pub(crate) mod callback;
pub(crate) mod flush_request;
pub(crate) mod flush_worker;

use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::mpsc::SyncSender;

use codeq::OffsetSize;
pub(crate) use flush_request::FlushStat;
pub(crate) use flush_request::WorkerRequest;
use log::info;

use crate::ChunkId;
use crate::Config;
use crate::Types;
use crate::WALRecord;
use crate::api::wal::WAL;
use crate::chunk::closed_chunk::ClosedChunk;
use crate::chunk::open_chunk::OpenChunk;
use crate::raft_log::log_data::LogData;
use crate::raft_log::state_machine::payload_cache::PayloadCache;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;
use crate::raft_log::wal::flush_request::SeqRequest;
use crate::raft_log::wal::flush_request::WriteRequest;
use crate::raft_log::wal::flush_worker::FileEntry;
use crate::raft_log::wal::flush_worker::FlushWorker;
use crate::types::Segment;

pub(crate) mod wal_record;

/// Write-ahead log implementation for the Raft log.
///
/// This WAL implementation manages both open and closed chunks of data.
/// An open chunk is actively being written to, while closed chunks are
/// immutable and may be used for reading historical data.
#[derive(Debug)]
pub(crate) struct RaftLogWAL<T>
where T: Types
{
    pub(crate) config: Arc<Config>,
    pub(crate) open: OpenChunk<T>,
    pub(crate) closed: BTreeMap<ChunkId, ClosedChunk<T>>,

    flush_tx: SyncSender<SeqRequest<T>>,

    /// The next sequence number to assign. Incremented on each `send_request`.
    /// Only accessed by the main thread, so a plain `u64` suffices.
    sent_seq: u64,

    /// Shared with `FlushWorker`; stores the highest completed seq.
    done_seq: Arc<AtomicU64>,
}

impl<T> RaftLogWAL<T>
where T: Types
{
    /// Creates a new RaftLogWAL instance.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the WAL
    /// * `closed` - Map of closed (immutable) chunks indexed by chunk ID
    /// * `open` - The currently active chunk that can be written to
    /// * `cache` - Cache for storing log payloads
    pub(crate) fn new(
        config: Arc<Config>,
        closed: BTreeMap<ChunkId, ClosedChunk<T>>,
        open: OpenChunk<T>,
        cache: Arc<RwLock<PayloadCache<T>>>,
    ) -> Self {
        let last_closed_chunk_state =
            closed.iter().last().map(|(_, c)| c.state.clone());

        let prev_last_log_id =
            last_closed_chunk_state.and_then(|s| s.last().cloned());

        let offset = open.chunk.global_start();
        let f = open.chunk.f.clone();

        let file_entry = FileEntry::new(offset, f, prev_last_log_id);

        let done_seq = Arc::new(AtomicU64::new(0));

        let (flush_tx, rx) = std::sync::mpsc::sync_channel(1024);
        let worker = FlushWorker::new(rx, file_entry, cache, done_seq.clone());

        worker.spawn();

        Self {
            config,
            open,
            closed,
            flush_tx,
            sent_seq: 0,
            done_seq,
        }
    }

    /// Wraps a `WorkerRequest` with an auto-incrementing seq and sends it to
    /// the FlushWorker.
    fn send_request(&mut self, req: WorkerRequest<T>) -> Result<(), io::Error> {
        self.sent_seq += 1;
        self.flush_tx
            .send(SeqRequest {
                seq: self.sent_seq,
                req,
            })
            .map_err(|e| {
                io::Error::other(format!("Failed to send request: {}", e))
            })
    }

    /// Block until the FlushWorker has processed all requests sent so far.
    ///
    /// Polls `done_seq` in a 1 ms sleep loop until it reaches `sent_seq`.
    /// This does NOT normalize the payload cache — call
    /// `RaftLog::drain_cache_evictable()` afterwards if a deterministic cache
    /// state is needed.
    pub(crate) fn wait_worker_idle(&self) {
        while self.done_seq.load(Ordering::Relaxed) < self.sent_seq {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    pub(crate) fn send_flush(
        &mut self,
        callback: T::Callback,
    ) -> Result<(), io::Error> {
        let data = self.open.take_pending_data();
        self.send_request(WorkerRequest::Write(WriteRequest {
            upto_offset: self.open.chunk.global_end(),
            data,
            sync: true,
            callback: Some(callback),
        }))
    }

    /// Requests removal of specified chunk files.
    ///
    /// # Arguments
    ///
    /// * `chunk_paths` - Paths of chunk files to be removed
    ///
    /// # Errors
    ///
    /// Returns an IO error if the remove request cannot be sent
    pub(crate) fn send_remove_chunks(
        &mut self,
        chunk_paths: Vec<String>,
    ) -> Result<(), io::Error> {
        self.send_request(WorkerRequest::RemoveChunks { chunk_paths })
    }

    #[allow(dead_code)]
    pub(crate) fn get_stat(&mut self) -> Result<Vec<FlushStat>, io::Error> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.send_get_stat(tx)?;
        rx.recv().map_err(|e| {
            io::Error::other(format!(
                "Failed to receive get state response: {}",
                e
            ))
        })
    }

    #[allow(dead_code)]
    pub(crate) fn send_get_stat(
        &mut self,
        callback: SyncSender<Vec<FlushStat>>,
    ) -> Result<(), io::Error> {
        self.send_request(WorkerRequest::GetFlushStat { tx: callback })
    }

    /// Checks if the current open chunk has reached its capacity.
    ///
    /// Returns true if either the maximum number of records or maximum chunk
    /// size is reached.
    pub(crate) fn is_open_chunk_full(&self) -> bool {
        self.open.chunk.records_count() >= self.config.chunk_max_records()
            || (self.open.chunk.chunk_size() as usize)
                >= self.config.chunk_max_size()
    }

    /// Attempts to close the current chunk if it's full and creates a new open
    /// chunk.
    ///
    /// # Arguments
    ///
    /// * `get_state` - Function to retrieve the current Raft log state. This
    ///   function is necessary because the state is not stored in the WAL.
    ///
    /// # Returns
    ///
    /// Returns Some(RaftLogState) if a chunk was closed, None otherwise
    ///
    /// # Errors
    ///
    /// Returns an IO error if chunk operations fail
    pub(crate) fn try_close_full_chunk(
        &mut self,
        get_state: impl FnOnce() -> RaftLogState<T>,
    ) -> Result<Option<RaftLogState<T>>, io::Error> {
        if !self.is_open_chunk_full() {
            return Ok(None);
        }

        let config = self.config.clone();
        let offset = self.open.chunk.last_segment().end();

        info!(
            "Closing full chunk: {}, open new: {}",
            self.open.chunk.chunk_id(),
            ChunkId(offset.0)
        );

        let state = get_state();

        let new_open = {
            let chunk_id = ChunkId(offset.0);
            OpenChunk::create(
                config,
                chunk_id,
                WALRecord::State(state.clone()),
            )?
        };

        let mut old_open = std::mem::replace(&mut self.open, new_open);

        let prev_pending_data = old_open.take_pending_data();
        if !prev_pending_data.is_empty() {
            self.send_request(WorkerRequest::Write(WriteRequest {
                upto_offset: offset.0,
                data: prev_pending_data,
                sync: true,
                callback: None,
            }))?;
        }

        self.send_request(WorkerRequest::AppendFile(FileEntry::new(
            offset.0,
            self.open.chunk.f.clone(),
            state.last().cloned(),
        )))?;

        let chunk = old_open.chunk;
        let closed_id = chunk.chunk_id();
        let closed = ClosedChunk::new(chunk, state.clone());
        self.closed.insert(closed_id, closed);
        Ok(Some(state))
    }

    /// Loads the payload for a given log entry.
    ///
    /// # Arguments
    ///
    /// * `log_data` - Metadata about the log entry to load
    ///
    /// # Returns
    ///
    /// Returns the log payload if found
    ///
    /// # Errors
    ///
    /// Returns an IO error if the chunk is not found or reading fails
    pub(crate) fn load_log_payload(
        &self,
        log_data: &LogData<T>,
    ) -> Result<T::LogPayload, io::Error> {
        let chunk_id = log_data.chunk_id;
        let segment = log_data.record_segment;

        // All logs in open chunk are cached.
        // See: payload_cache.set_last_evictable()

        let record = {
            let closed = self.closed.get(&chunk_id).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "Chunk not found: {}; when:(open cache-miss read)",
                        chunk_id
                    ),
                )
            })?;
            closed.chunk.read_record(segment)?
        };

        if let WALRecord::Append(log_id, payload) = record {
            debug_assert_eq!(log_id, log_data.log_id);
            Ok(payload)
        } else {
            panic!("Expect Record::Append but: {:?}", record);
        }
    }
}

impl<T> WAL<WALRecord<T>> for RaftLogWAL<T>
where T: Types
{
    fn append(&mut self, rec: &WALRecord<T>) -> Result<(), io::Error> {
        self.open.append_record(rec)?;
        Ok(())
    }

    fn last_segment(&self) -> Segment {
        self.open.chunk.last_segment()
    }
}
