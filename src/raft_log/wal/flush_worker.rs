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
        // TODO: exhaust the queue and batch many flushes together.
        for req in self.rx.iter() {
            match req {
                FlushRequest::AppendFile { offset, f } => {
                    self.files.push(FileEntry::new(offset, f));
                }
                FlushRequest::Flush {
                    upto_offset: offset,
                    callback,
                } => {
                    let res = Self::sync_all_files(&mut self.files, offset);
                    callback.send(res);
                }
                FlushRequest::GetFlushStat { tx: callback } => {
                    let stat = self
                        .files
                        .iter()
                        .map(|f| (f.starting_offset, f.sync_id))
                        .collect();
                    let _ = callback.send(stat);
                }
            }
        }
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
