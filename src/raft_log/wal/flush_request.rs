use std::fs::File;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;

use crate::Types;

pub(crate) enum FlushRequest<T: Types> {
    /// Append a new file that will be need to be sync.
    AppendFile {
        /// The global offset this file starts
        offset: u64,
        f: Arc<File>,
    },
    /// Sync all files in order.
    Flush {
        /// fdatasync the data in WAL at least upto this offset, inclusive.
        /// This is filled with current global offset when this fdatasync is
        /// called.
        upto_offset: u64,
        callback: T::Callback,
    },

    /// For debug, return a list of offset and sync id of all files.
    #[allow(dead_code)]
    GetFlushStat { tx: SyncSender<Vec<(u64, u64)>> },
}
