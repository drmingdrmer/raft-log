use std::fmt;
use std::fs::File;
use std::io;
use std::io::IoSlice;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;
use std::time::Instant;

use log::debug;
use log::info;

use crate::ChunkId;
use crate::WalTypes;
use crate::raft_log::wal::atomic_flush_metrics::AtomicFlushMetrics;
use crate::raft_log::wal::batch_metrics::BatchMetrics;
use crate::raft_log::wal::callback::Callback;
use crate::raft_log::wal::file_persisted::ChunkPersisted;
use crate::raft_log::wal::file_persisted::ChunkPersistedCallback;
use crate::raft_log::wal::flush_request::FlushStat;
use crate::raft_log::wal::flush_request::SeqRequest;
use crate::raft_log::wal::flush_request::WorkerRequest;
use crate::raft_log::wal::queued_write::QueuedWrite;

pub(crate) struct FileEntry<W>
where W: WalTypes
{
    pub(crate) starting_offset: u64,
    pub(crate) f: Arc<File>,

    /// Called after this file has been successfully synced.
    ///
    /// Receives the file, its starting offset, and the synced offset.
    /// The callback may be called multiple times.
    pub(crate) on_persisted: ChunkPersistedCallback<W>,
    /// for debug
    pub(crate) sync_id: u64,
}

impl<W> fmt::Display for FileEntry<W>
where W: WalTypes
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FileEntry{{ starting_offset: {}, sync_id: {} }}",
            ChunkId(self.starting_offset),
            self.sync_id
        )
    }
}

impl<W> fmt::Debug for FileEntry<W>
where W: WalTypes
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileEntry")
            .field("starting_offset", &ChunkId(self.starting_offset))
            .field("sync_id", &self.sync_id)
            .finish()
    }
}

impl<W> FileEntry<W>
where W: WalTypes
{
    pub(crate) fn new(
        starting_offset: u64,
        f: Arc<File>,
        on_persisted: ChunkPersistedCallback<W>,
    ) -> Self {
        Self {
            starting_offset,
            f,
            on_persisted,
            sync_id: 0,
        }
    }
}

struct WriteBatch<W>
where W: WalTypes
{
    writes: Vec<QueuedWrite<W>>,
    max_seq: u64,
    last_non_flush: Option<SeqRequest<W>>,
    max_size: usize,
}

impl<W> WriteBatch<W>
where W: WalTypes
{
    fn new(max_size: usize) -> Self {
        Self {
            writes: Vec::with_capacity(max_size),
            max_seq: 0,
            last_non_flush: None,
            max_size,
        }
    }

    fn push_seq_request(&mut self, seq_req: SeqRequest<W>) -> bool {
        let SeqRequest {
            seq,
            queued_at,
            req,
        } = seq_req;

        if let WorkerRequest::Write(write) = req {
            self.max_seq = self.max_seq.max(seq);
            self.writes.push(QueuedWrite { queued_at, write });
            true
        } else {
            self.last_non_flush = Some(SeqRequest {
                seq,
                queued_at,
                req,
            });
            false
        }
    }
}

pub(crate) struct FlushWorker<W>
where W: WalTypes
{
    rx: Receiver<SeqRequest<W>>,
    files: Vec<FileEntry<W>>,
    metrics: Arc<AtomicFlushMetrics>,
    flush_batch_wait: Duration,
    flush_batch_max_items: usize,
    /// The highest completed request sequence number.
    ///
    /// Updated (with `Relaxed` ordering) after processing each request or
    /// batch. The main thread polls this to implement `wait_worker_idle()`.
    /// `Relaxed` is sufficient because this value is only a progress counter;
    /// request side effects provide their own synchronization.
    done_seq: Arc<AtomicU64>,
}

impl<W> FlushWorker<W>
where W: WalTypes
{
    /// When starting, there is at most one open chunk file that is not sync.
    pub(crate) fn spawn(self) {
        std::thread::Builder::new()
            .name("raft_log_wal_flush_worker".to_string())
            .spawn(move || {
                self.run();
            })
            .expect("Failed to start sync worker thread");
    }

    pub(crate) fn new(
        rx: Receiver<SeqRequest<W>>,
        file_entry: FileEntry<W>,
        done_seq: Arc<AtomicU64>,
        metrics: Arc<AtomicFlushMetrics>,
        flush_batch_wait: Duration,
        flush_batch_max_items: usize,
    ) -> Self {
        Self {
            rx,
            files: vec![file_entry],
            metrics,
            flush_batch_wait,
            flush_batch_max_items,
            done_seq,
        }
    }

    fn run(self) {
        let res = self.run_inner();
        if let Err(e) = res {
            log::error!("FlushWorker failed: {}", e);
        }
    }

    fn run_inner(mut self) -> Result<(), io::Error> {
        loop {
            // Write requests should be batched to maximize throughput.
            let mut batch = WriteBatch::new(self.flush_batch_max_items);

            let req = self.rx.recv();
            let Ok(seq_req) = req else {
                log::info!("FlushWorker input channel closed, quit");
                return Ok(());
            };

            if !batch.push_seq_request(seq_req) {
                let Some(SeqRequest { seq, req, .. }) =
                    batch.last_non_flush.take()
                else {
                    unreachable!("non-write request must be stored");
                };
                self.handle_non_flush_request(req)?;
                self.done_seq.store(seq, Ordering::Relaxed);
                continue;
            }

            let group_wait = self.collect_write_batch(&mut batch);

            debug!("batched write: {}", batch.writes.len());

            let sync_result = {
                // TODO: possible to use write_all_vectored()?

                let mut last_file: &File = &self.files.last().unwrap().f;
                let batch_start = Instant::now();
                let mut batch_metrics =
                    BatchMetrics::new(batch.writes.len(), group_wait);
                for w in &batch.writes {
                    batch_metrics.record_queued_write(batch_start, w);
                }

                let write_start = Instant::now();
                write_batch_vectored(&mut last_file, &batch.writes)?;
                batch_metrics.record_write_time(write_start);

                let need_sync = batch.writes.iter().any(|w| w.write.sync);

                let sync_result = if need_sync {
                    let upto_offset =
                        batch.writes.last().unwrap().write.upto_offset;
                    let sync_start = Instant::now();
                    let res = self.sync_data_files(upto_offset);
                    batch_metrics.record_sync_time(sync_start);
                    if let Err(ref e) = res {
                        log::error!(
                            "Failed to flush upto offset {}: {}",
                            upto_offset,
                            e
                        );
                    }
                    res
                } else {
                    Ok(())
                };

                batch_metrics.record_batch_time(batch_start);
                self.metrics.record_batch(batch_metrics);

                sync_result
            };

            let WriteBatch {
                writes,
                mut max_seq,
                last_non_flush,
                ..
            } = batch;

            for w in writes {
                if let Some(cb) = w.write.callback {
                    match &sync_result {
                        Ok(()) => cb.send(Ok(())),
                        Err(e) => {
                            cb.send(Err(io::Error::new(
                                e.kind(),
                                e.to_string(),
                            )));
                        }
                    }
                }
            }

            // Handle the last non-flush request
            if let Some(SeqRequest {
                seq: nf_seq,
                req: last,
                ..
            }) = last_non_flush
            {
                self.handle_non_flush_request(last)?;
                max_seq = max_seq.max(nf_seq);
            }

            self.done_seq.store(max_seq, Ordering::Relaxed);
        }
    }

    fn collect_write_batch(&self, batch: &mut WriteBatch<W>) -> Duration {
        let loop_started_at = Instant::now();
        let loop_deadline = loop_started_at + self.flush_batch_wait;

        while batch.last_non_flush.is_none()
            && batch.writes.len() < batch.max_size
        {
            let now = Instant::now();
            if loop_deadline <= now {
                break;
            }

            let remaining = loop_deadline - now;
            match self.rx.recv_timeout(remaining) {
                Ok(seq_req) => {
                    if !batch.push_seq_request(seq_req) {
                        break;
                    }
                }
                Err(
                    RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected,
                ) => break,
            }
        }

        loop_started_at.elapsed()
    }

    fn handle_non_flush_request(
        &mut self,
        req: WorkerRequest<W>,
    ) -> Result<(), io::Error> {
        match req {
            WorkerRequest::AppendFile(file_entry) => {
                info!("FlushWorker: AppendFile: {}", file_entry);
                self.files.push(file_entry);
            }
            WorkerRequest::Write(_) => {
                unreachable!("Write request should be handled in run()");
            }
            WorkerRequest::GetFlushStat { tx } => {
                let stat = self
                    .files
                    .iter()
                    .map(|f| FlushStat {
                        starting_offset: f.starting_offset,
                        sync_id: f.sync_id,
                        ino: f.f.metadata().unwrap().ino(),
                    })
                    .collect();
                let _ = tx.send(stat);
            }
            WorkerRequest::RemoveChunks { chunk_paths } => {
                info!("FlushWorker: RemoveChunks: {:?}", chunk_paths);
                for path in chunk_paths {
                    std::fs::remove_file(path)?;
                }
            }
        }

        Ok(())
    }

    pub fn sync_data_files(&mut self, offset: u64) -> Result<(), io::Error> {
        let files = &mut self.files;

        if files.is_empty() {
            return Ok(());
        }

        // Append-only WAL flushes only need file data durability. Metadata
        // changes such as recovery truncation are synchronized at the call
        // site that performs the metadata update.
        while files.len() > 1 {
            let f = files.remove(0);
            f.f.sync_data()?;
        }

        let f = &mut files[0];
        f.f.sync_data()?;
        f.on_persisted.call(ChunkPersisted {
            file: f.f.clone(),
            starting_offset: f.starting_offset,
            synced_offset: offset,
        });
        f.sync_id = offset;

        Ok(())
    }
}

fn write_batch_vectored<W>(
    file: &mut &File,
    writes: &[QueuedWrite<W>],
) -> Result<(), io::Error>
where
    W: WalTypes,
{
    const MAX_VECTORED_WRITE_SLICES: usize = 1024;

    for chunk in writes.chunks(MAX_VECTORED_WRITE_SLICES) {
        let mut slices = chunk
            .iter()
            .filter(|w| !w.write.data.is_empty())
            .map(|w| w.write.data.as_slice())
            .collect::<Vec<_>>();

        if !slices.is_empty() {
            write_all_vectored(file, &mut slices)?;
        }
    }

    Ok(())
}

fn write_all_vectored(
    file: &mut &File,
    buffers: &mut [&[u8]],
) -> Result<(), io::Error> {
    let mut start = 0;

    while start < buffers.len() {
        let io_slices = buffers[start..]
            .iter()
            .map(|buffer| IoSlice::new(buffer))
            .collect::<Vec<_>>();

        let mut written = match file.write_vectored(&io_slices) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        while written > 0 {
            let len = buffers[start].len();
            if written < len {
                buffers[start] = &buffers[start][written..];
                break;
            }

            written -= len;
            start += 1;
            if start == buffers.len() {
                break;
            }
        }
    }

    Ok(())
}
