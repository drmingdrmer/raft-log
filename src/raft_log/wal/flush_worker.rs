use std::fs::File;
use std::io;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;

use crate::raft_log::wal::callback::Callback;
use crate::raft_log::wal::flush_request::FlushRequest;
use crate::Types;

pub(crate) struct FileEntry {
    pub(crate) starting_offset: u64,
    pub(crate) f: Arc<File>,
    /// for debug
    pub(crate) sync_id: u64,
}

impl FileEntry {
    pub(crate) fn new(starting_offset: u64, f: Arc<File>) -> Self {
        Self {
            starting_offset,
            f,
            sync_id: 0,
        }
    }
}

pub(crate) struct FlushWorker<T: Types> {
    rx: Receiver<FlushRequest<T>>,
    files: Vec<FileEntry>,
}

impl<T: Types> FlushWorker<T> {
    /// When starting, there is at most one open chunk file that is not sync.
    pub(crate) fn start_flush_worker(
        offset: u64,
        f: Arc<File>,
    ) -> SyncSender<FlushRequest<T>> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1024);

        let worker = FlushWorker::new(rx, offset, f);

        std::thread::Builder::new()
            .name("raft_log_wal_flush_worker".to_string())
            .spawn(move || {
                worker.run();
            })
            .expect("Failed to start sync worker thread");

        tx
    }

    pub(crate) fn new(
        rx: Receiver<FlushRequest<T>>,
        offset: u64,
        f: Arc<File>,
    ) -> Self {
        Self {
            rx,
            files: vec![FileEntry::new(offset, f)],
        }
    }

    fn run(mut self) {
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

            {
                let last_flush = batch.last().unwrap();
                let res = Self::sync_all_files(
                    &mut self.files,
                    last_flush.upto_offset,
                );

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
            FlushRequest::AppendFile { offset, f } => {
                self.files.push(FileEntry::new(offset, f));
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
                for path in chunk_paths {
                    std::fs::remove_file(path)?;
                }
            }
        }

        Ok(())
    }

    pub fn sync_all_files(
        files: &mut Vec<FileEntry>,
        offset: u64,
    ) -> Result<(), io::Error> {
        if files.is_empty() {
            return Ok(());
        }

        while files.len() > 1 {
            let f = files.remove(0);
            f.f.sync_data()?;
        }

        files[0].f.sync_data()?;
        files[0].sync_id = offset;

        Ok(())
    }
}
