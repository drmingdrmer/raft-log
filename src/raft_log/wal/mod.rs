pub(crate) mod atomic_flush_metrics;
pub(crate) mod batch_metrics;
pub(crate) mod callback;
pub(crate) mod file_persisted;
pub(crate) mod flush_request;
pub(crate) mod flush_worker;
pub(crate) mod queued_write;

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

use codeq::OffsetSize;
pub(crate) use flush_request::FlushStat;
pub(crate) use flush_request::WorkerRequest;
use log::info;

use crate::ChunkId;
use crate::Config;
use crate::RaftLogRecord;
use crate::WALRecord;
use crate::WalTypes;
use crate::api::state_machine::StateMachine;
use crate::api::wal::WAL;
use crate::chunk::closed_chunk::ClosedChunk;
use crate::chunk::open_chunk::OpenChunk;
use crate::raft_log::stat::FlushMetrics;
use crate::raft_log::wal::atomic_flush_metrics::AtomicFlushMetrics;
use crate::raft_log::wal::file_persisted::ChunkPersistedCallback;
use crate::raft_log::wal::flush_request::SeqRequest;
use crate::raft_log::wal::flush_request::WriteRequest;
use crate::raft_log::wal::flush_worker::FileEntry;
use crate::raft_log::wal::flush_worker::FlushWorker;
use crate::types::Segment;

pub(crate) mod wal_record;

pub(crate) use crate::raft_log::wal::file_persisted::ChunkPersistedFn;

/// Write-ahead log implementation for the Raft log.
///
/// This WAL implementation manages both open and closed chunks of data.
/// An open chunk is actively being written to, while closed chunks are
/// immutable and may be used for reading historical data.
pub(crate) struct RaftLogWAL<W>
where W: WalTypes
{
    pub(crate) config: Arc<Config>,
    pub(crate) open: OpenChunk<RaftLogRecord<W>>,
    pub(crate) closed: BTreeMap<ChunkId, ClosedChunk<W>>,

    /// Sends user write operations to the flush worker.
    ///
    /// Each write operation may carry its own callback, defined by
    /// `W::Callback`.
    flush_tx: SyncSender<SeqRequest<W>>,

    /// File-level callback invoked after fsync.
    ///
    /// This callback is called once for each synced chunk file.
    on_chunk_persisted: ChunkPersistedFn<W>,

    /// The next sequence number to assign. Incremented on each `send_request`.
    /// Only accessed by the main thread, so a plain `u64` suffices.
    sent_seq: u64,

    /// Shared with `FlushWorker`; stores the highest completed seq.
    done_seq: Arc<AtomicU64>,

    /// Shared with `FlushWorker`; stores aggregated flush metrics.
    flush_metrics: Arc<AtomicFlushMetrics>,
}

impl<W> fmt::Debug for RaftLogWAL<W>
where W: WalTypes
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RaftLogWAL")
            .field("config", &self.config)
            .field("open", &self.open)
            .field("closed", &self.closed)
            .field("sent_seq", &self.sent_seq)
            .field("done_seq", &self.done_seq)
            .field("flush_metrics", &self.flush_metrics)
            .finish_non_exhaustive()
    }
}

impl<W> RaftLogWAL<W>
where W: WalTypes
{
    /// Creates a new RaftLogWAL instance.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the WAL
    /// * `closed` - Map of closed (immutable) chunks indexed by chunk ID
    /// * `open` - The currently active chunk that can be written to
    /// * `on_chunk_persisted` - Callback invoked after chunk data is persisted
    pub(crate) fn new(
        config: Arc<Config>,
        closed: BTreeMap<ChunkId, ClosedChunk<W>>,
        open: OpenChunk<WALRecord<W>>,
        on_chunk_persisted: ChunkPersistedFn<W>,
    ) -> Self {
        let prev_checkpoint =
            closed.iter().last().map(|(_, c)| c.state.clone());

        let offset = open.chunk.global_start();
        let f = open.chunk.f.clone();

        let file_entry = FileEntry::new(
            offset,
            f,
            ChunkPersistedCallback::new(
                on_chunk_persisted.clone(),
                prev_checkpoint,
            ),
        );

        let done_seq = Arc::new(AtomicU64::new(0));
        let flush_metrics = Arc::new(AtomicFlushMetrics::default());

        let (flush_tx, rx) = std::sync::mpsc::sync_channel(1024);
        let worker = FlushWorker::new(
            rx,
            file_entry,
            done_seq.clone(),
            flush_metrics.clone(),
            config.flush_batch_wait(),
            config.flush_batch_max_items(),
        );

        worker.spawn();

        Self {
            config,
            open,
            closed,
            flush_tx,
            on_chunk_persisted,
            sent_seq: 0,
            done_seq,
            flush_metrics,
        }
    }

    /// Wraps a `WorkerRequest` with an auto-incrementing seq and sends it to
    /// the FlushWorker.
    fn send_request(&mut self, req: WorkerRequest<W>) -> Result<(), io::Error> {
        self.sent_seq += 1;
        self.flush_tx
            .send(SeqRequest {
                seq: self.sent_seq,
                queued_at: Instant::now(),
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

    pub(crate) fn flush_metrics(&self) -> FlushMetrics {
        self.flush_metrics.snapshot()
    }

    /// Hand the pending data buffer to the worker for writing.
    ///
    /// Drains `OpenChunk::pending_data` and packages it as a `WriteRequest`.
    /// The worker always writes the bytes to the OS file. When `sync` is
    /// `true` it also calls `fsync` so the data is on stable storage; when
    /// `sync` is `false` it skips the fsync and durability is deferred to
    /// the next sync write that lands in the same or a later batch.
    pub(crate) fn send_pending(
        &mut self,
        sync: bool,
        callback: Option<W::Callback>,
    ) -> Result<(), io::Error> {
        let data = self.open.take_pending_data();
        self.send_request(WorkerRequest::Write(WriteRequest {
            upto_offset: self.open.chunk.global_end(),
            data,
            sync,
            callback,
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
    /// * `state_machine` - The state machine that provides the checkpoint to
    ///   store at the start of the next chunk.
    ///
    /// # Returns
    ///
    /// Returns the checkpoint if a chunk was closed, None otherwise.
    ///
    /// # Errors
    ///
    /// Returns an IO error if chunk operations fail
    pub(crate) fn try_close_full_chunk<SM>(
        &mut self,
        state_machine: &SM,
    ) -> Result<Option<W::Checkpoint>, io::Error>
    where
        SM: StateMachine<W>,
    {
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

        let checkpoint = state_machine.checkpoint();

        let new_open = {
            let chunk_id = ChunkId(offset.0);
            OpenChunk::create(
                config,
                chunk_id,
                RaftLogRecord::Checkpoint(checkpoint.clone()),
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

        let checkpoint = Arc::new(checkpoint);

        self.send_request(WorkerRequest::AppendFile(FileEntry::new(
            offset.0,
            self.open.chunk.f.clone(),
            ChunkPersistedCallback::new(
                self.on_chunk_persisted.clone(),
                Some(checkpoint.clone()),
            ),
        )))?;

        let chunk = old_open.chunk;
        let closed_id = chunk.chunk_id();
        let closed = ClosedChunk::new(chunk, checkpoint.clone());
        self.closed.insert(closed_id, closed);
        Ok(Some(checkpoint.as_ref().clone()))
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
    pub(crate) fn load_record(
        &self,
        chunk_id: &ChunkId,
        segment: Segment,
    ) -> Result<WALRecord<W>, io::Error> {
        // All logs in the open chunk are served before this fallback.

        let record = {
            let closed = self.closed.get(chunk_id).ok_or_else(|| {
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

        Ok(record)
    }
}

impl<W> WAL<WALRecord<W>> for RaftLogWAL<W>
where W: WalTypes
{
    fn append(&mut self, rec: &RaftLogRecord<W>) -> Result<(), io::Error> {
        self.open.append_record(rec)?;
        Ok(())
    }

    fn last_segment(&self) -> Segment {
        self.open.chunk.last_segment()
    }
}
