use std::fmt;
use std::fs::File;
use std::io;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::mpsc::Receiver;

use log::debug;
use log::info;

use crate::ChunkId;
use crate::Types;
use crate::raft_log::state_machine::payload_cache::PayloadCache;
use crate::raft_log::wal::callback::Callback;
use crate::raft_log::wal::flush_request::FlushRequest;

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

pub(crate) struct FlushWorker<T: Types> {
    rx: Receiver<FlushRequest<T>>,
    files: Vec<FileEntry<T>>,
    cache: Arc<RwLock<PayloadCache<T>>>,
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
        rx: Receiver<FlushRequest<T>>,
        file_entry: FileEntry<T>,
        cache: Arc<RwLock<PayloadCache<T>>>,
    ) -> Self {
        Self {
            rx,
            files: vec![file_entry],
            cache,
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
            let req = self.rx.recv();
            let Ok(req) = req else {
                log::info!("FlushWorker input channel closed, quit");
                return Ok(());
            };

            let FlushRequest::Flush(flush) = req else {
                self.handle_non_flush_request(req)?;
                continue;
            };

            // Flush request should be batched to maximize throughput.

            let batch_size = 1024;

            let mut batch = Vec::with_capacity(batch_size);
            batch.push(flush);
            let mut last_non_flush = None;

            for req in self.rx.try_iter().take(batch_size) {
                if let FlushRequest::Flush(f) = req {
                    batch.push(f);
                } else {
                    last_non_flush = Some(req);
                    break;
                };
            }

            debug!("batched flush: {}", batch.len());

            {
                let last_flush = batch.last().unwrap();
                let res = self.sync_all_files(last_flush.upto_offset);

                match res {
                    Ok(_) => {
                        for x in batch {
                            x.callback.send(Ok(()));
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to flush upto offset {}: {}",
                            last_flush.upto_offset,
                            e
                        );

                        for x in batch {
                            x.callback.send(Err(io::Error::new(
                                e.kind(),
                                e.to_string(),
                            )));
                        }
                    }
                }
            }

            // Handle the last non-flush request
            if let Some(last) = last_non_flush {
                self.handle_non_flush_request(last)?;
            }
        }
    }

    fn handle_non_flush_request(
        &mut self,
        req: FlushRequest<T>,
    ) -> Result<(), io::Error> {
        match req {
            FlushRequest::AppendFile(file_entry) => {
                info!("FlushWorker: AppendFile: {}", file_entry);
                self.files.push(file_entry);
            }
            FlushRequest::Flush(_) => {
                unreachable!("Flush request should be handled in run()");
            }
            FlushRequest::GetFlushStat { tx } => {
                let stat = self
                    .files
                    .iter()
                    .map(|f| (f.starting_offset, f.sync_id))
                    .collect();
                let _ = tx.send(stat);
            }
            FlushRequest::RemoveChunks { chunk_paths } => {
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
