use std::fmt;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

use crate::WalTypes;
use crate::raft_log::wal::flush_worker::FileEntry;

/// A `WorkerRequest` tagged with a monotonically increasing sequence number.
///
/// The main thread assigns an incrementing `seq` to every request it sends.
/// After processing a request, the FlushWorker stores the highest completed
/// seq into a shared `AtomicU64`, allowing the main thread to wait until all
/// sent requests have been processed.
pub(crate) struct SeqRequest<W>
where W: WalTypes
{
    pub(crate) seq: u64,
    pub(crate) queued_at: Instant,
    pub(crate) req: WorkerRequest<W>,
}

impl<W> fmt::Debug for SeqRequest<W>
where W: WalTypes
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SeqRequest")
            .field("seq", &self.seq)
            .field("queued_at", &self.queued_at)
            .finish_non_exhaustive()
    }
}

pub(crate) struct WriteRequest<W>
where W: WalTypes
{
    pub(crate) upto_offset: u64,
    pub(crate) data: Vec<u8>,
    pub(crate) sync: bool,
    pub(crate) callback: Option<W::Callback>,
}

impl<W> fmt::Debug for WriteRequest<W>
where W: WalTypes
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WriteRequest")
            .field("upto_offset", &self.upto_offset)
            .field("data_len", &self.data.len())
            .field("sync", &self.sync)
            .field("has_callback", &self.callback.is_some())
            .finish()
    }
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

pub(crate) enum WorkerRequest<W>
where W: WalTypes
{
    /// Append a new file that will be need to be sync.
    AppendFile(FileEntry<W>),

    /// Remove chunks that have been purged.
    ///
    /// This job must be done in FlushWorker to ensure it is after the
    /// corresponding purge record is flushed.
    RemoveChunks { chunk_paths: Vec<String> },

    /// Write data, and optionally sync all files.
    Write(WriteRequest<W>),

    /// For debug, return a list of offset and sync id of all files.
    #[allow(dead_code)]
    GetFlushStat { tx: SyncSender<Vec<FlushStat>> },
}

impl<W> fmt::Debug for WorkerRequest<W>
where W: WalTypes
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkerRequest::AppendFile(file_entry) => {
                f.debug_tuple("AppendFile").field(file_entry).finish()
            }
            WorkerRequest::RemoveChunks { chunk_paths } => f
                .debug_struct("RemoveChunks")
                .field("chunk_paths", chunk_paths)
                .finish(),
            WorkerRequest::Write(write) => {
                f.debug_tuple("Write").field(write).finish()
            }
            WorkerRequest::GetFlushStat { .. } => {
                f.debug_struct("GetFlushStat").finish_non_exhaustive()
            }
        }
    }
}
