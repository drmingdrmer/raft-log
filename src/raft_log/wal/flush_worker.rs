use std::fmt;
use std::fs::File;
use std::io;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;
use std::time::Instant;

use log::debug;
use log::info;

use crate::ChunkId;
use crate::Types;
use crate::raft_log::state_machine::payload_cache::PayloadCache;
use crate::raft_log::wal::atomic_flush_metrics::AtomicFlushMetrics;
use crate::raft_log::wal::batch_metrics::BatchMetrics;
use crate::raft_log::wal::callback::Callback;
use crate::raft_log::wal::flush_request::FlushStat;
use crate::raft_log::wal::flush_request::SeqRequest;
use crate::raft_log::wal::flush_request::WorkerRequest;
use crate::raft_log::wal::queued_write::QueuedWrite;

pub(crate) struct FileEntry<T: Types> {
    pub(crate) starting_offset: u64,
    pub(crate) f: Arc<File>,

    /// The first log id in this file, also the last log id in the previous
    /// chunk file.
    pub(crate) prev_last_log_id: Option<T::LogId>,
    /// for debug
    pub(crate) sync_id: u64,
}

impl<T> fmt::Display for FileEntry<T>
where T: Types
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FileEntry{{ starting_offset: {}, prev_last_log_id: {:?} sync_id: {} }}",
            ChunkId(self.starting_offset),
            self.prev_last_log_id,
            self.sync_id
        )
    }
}

impl<T: Types> FileEntry<T> {
    /// `last_log_id`: the last log id in the previous chunk file. It is used to
    /// set the cache eviction boundary.
    pub(crate) fn new(
        starting_offset: u64,
        f: Arc<File>,
        prev_last_log_id: Option<T::LogId>,
    ) -> Self {
        Self {
            starting_offset,
            f,
            prev_last_log_id,
            sync_id: 0,
        }
    }
}

struct WriteBatch<T: Types> {
    writes: Vec<QueuedWrite<T>>,
    max_seq: u64,
    last_non_flush: Option<SeqRequest<T>>,
    max_size: usize,
}

impl<T: Types> WriteBatch<T> {
    fn new(max_size: usize) -> Self {
        Self {
            writes: Vec::with_capacity(max_size),
            max_seq: 0,
            last_non_flush: None,
            max_size,
        }
    }

    fn push_seq_request(&mut self, seq_req: SeqRequest<T>) -> bool {
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

pub(crate) struct FlushWorker<T: Types> {
    rx: Receiver<SeqRequest<T>>,
    files: Vec<FileEntry<T>>,
    cache: Arc<RwLock<PayloadCache<T>>>,
    metrics: Arc<AtomicFlushMetrics>,
    flush_batch_wait: Duration,
    /// The highest completed request sequence number.
    ///
    /// Updated (with `Relaxed` ordering) after processing each request or
    /// batch. The main thread polls this to implement `wait_worker_idle()`.
    /// `Relaxed` is sufficient because the actual data synchronization is
    /// provided by the `RwLock` on `PayloadCache`.
    done_seq: Arc<AtomicU64>,
}

impl<T: Types> FlushWorker<T> {
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
        rx: Receiver<SeqRequest<T>>,
        file_entry: FileEntry<T>,
        cache: Arc<RwLock<PayloadCache<T>>>,
        done_seq: Arc<AtomicU64>,
        metrics: Arc<AtomicFlushMetrics>,
        flush_batch_wait: Duration,
    ) -> Self {
        Self {
            rx,
            files: vec![file_entry],
            cache,
            metrics,
            flush_batch_wait,
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
            let batch_size = 1024;
            let mut batch = WriteBatch::new(batch_size);

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
                for w in &batch.writes {
                    if !w.write.data.is_empty() {
                        last_file.write_all(&w.write.data)?;
                    }
                }
                batch_metrics.record_write_time(write_start);

                let need_sync = batch.writes.iter().any(|w| w.write.sync);

                let sync_result = if need_sync {
                    let upto_offset =
                        batch.writes.last().unwrap().write.upto_offset;
                    let sync_start = Instant::now();
                    let res = self.sync_all_files(upto_offset);
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

    fn collect_write_batch(&self, batch: &mut WriteBatch<T>) -> Duration {
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
        req: WorkerRequest<T>,
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

    pub fn sync_all_files(&mut self, offset: u64) -> Result<(), io::Error> {
        let files = &mut self.files;

        if files.is_empty() {
            return Ok(());
        }

        while files.len() > 1 {
            let f = files.remove(0);
            f.f.sync_data()?;
        }

        // The second last and before are all closed,
        // When sync-ed, the logs in the cache can be evicted

        let f = &mut files[0];

        {
            let mut cache = self.cache.write().unwrap();
            cache.set_last_evictable(f.prev_last_log_id.clone());
        }

        files[0].f.sync_data()?;
        files[0].sync_id = offset;

        Ok(())
    }
}
