use std::sync::mpsc::SyncSender;

use crate::Types;
use crate::raft_log::wal::flush_worker::FileEntry;

pub(crate) struct WriteRequest<T: Types> {
    pub(crate) upto_offset: u64,
    pub(crate) data: Vec<u8>,
    pub(crate) sync: bool,
    pub(crate) callback: Option<T::Callback>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct FlushStat {
    pub(crate) starting_offset: u64,
    pub(crate) sync_id: u64,
    pub(crate) ino: u64,
}

impl FlushStat {
    #[allow(dead_code)]
    pub(crate) fn offset_sync_id(&self) -> (u64, u64) {
        (self.starting_offset, self.sync_id)
    }
}

pub(crate) enum WorkerRequest<T: Types> {
    /// Append a new file that will be need to be sync.
    AppendFile(FileEntry<T>),

    /// Remove chunks that have been purged.
    ///
    /// This job must be done in FlushWorker to ensure it is after the
    /// corresponding purge record is flushed.
    RemoveChunks { chunk_paths: Vec<String> },

    /// Write data, and optionally sync all files.
    Write(WriteRequest<T>),

    /// For debug, return a list of offset and sync id of all files.
    #[allow(dead_code)]
    GetFlushStat { tx: SyncSender<Vec<FlushStat>> },
}
