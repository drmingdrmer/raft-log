use std::io;
use std::sync::Arc;

use chunked_wal::ChunkedWal;
use chunked_wal::WalLock;

use crate::ChunkId;
use crate::Config;
use crate::RaftLog;
use crate::RaftLogRecord;
use crate::RaftWalTypes;
use crate::Types;
use crate::raft_log::dump_api::DumpApi;
use crate::types::Segment;

/// A dump utility that reads WAL records from disk.
///
/// It acquires an exclusive lock on the directory to prevent concurrent writes
/// while reading.
pub struct Dump<T> {
    config: Arc<Config>,

    /// Holds the WAL directory lock while reading files from disk.
    _wal_lock: WalLock,

    _p: std::marker::PhantomData<T>,
}

impl<T: Types> DumpApi<T> for Dump<T> {
    /// Reads all WAL records from disk and passes them to the provided callback
    /// function.
    ///
    /// The callback receives:
    /// - `chunk_id`: The ID of the chunk containing the record
    /// - `index`: The 0-based index of the record within its chunk
    /// - `result`: The result containing either the record data or an IO error
    ///
    /// # Errors
    /// Returns an IO error if reading the chunks fails or if the callback
    /// returns an error.
    fn write_with<D>(&self, write_record: D) -> Result<(), io::Error>
    where D: FnMut(
            ChunkId,
            u64,
            Result<(Segment, RaftLogRecord<T>), io::Error>,
        ) -> Result<(), io::Error> {
        ChunkedWal::<RaftWalTypes<T>>::dump_records(
            &self.config.wal,
            &self._wal_lock,
            write_record,
        )
    }
}

/// A dump utility that reads WAL records from an existing RaftLog instance.
///
/// Unlike [`Dump`], this does not acquire a directory lock since it operates on
/// an already initialized RaftLog.
pub struct RefDump<'a, T: Types> {
    pub(crate) raft_log: &'a RaftLog<T>,
}

impl<T: Types> DumpApi<T> for RefDump<'_, T> {
    /// Reads all WAL records from the RaftLog and passes them to the provided
    /// callback function.
    ///
    /// The callback receives:
    /// - `chunk_id`: The ID of the chunk containing the record
    /// - `index`: The 0-based index of the record within its chunk
    /// - `result`: The result containing either the record data or an IO error
    ///
    /// # Errors
    /// Returns an IO error if reading the chunks fails or if the callback
    /// returns an error.
    fn write_with<D>(&self, write_record: D) -> Result<(), io::Error>
    where D: FnMut(
            ChunkId,
            u64,
            Result<(Segment, RaftLogRecord<T>), io::Error>,
        ) -> Result<(), io::Error> {
        self.raft_log.wal.dump_loaded_records(write_record)
    }
}

impl<T: Types> Dump<T> {
    /// Creates a new Dump instance with the given configuration.
    ///
    /// # Errors
    /// Returns an IO error if acquiring the directory lock fails.
    pub fn new(config: Arc<Config>) -> Result<Self, io::Error> {
        let wal_lock =
            ChunkedWal::<RaftWalTypes<T>>::acquire_lock(&config.wal)?;

        Ok(Self {
            config,
            _wal_lock: wal_lock,
            _p: std::marker::PhantomData,
        })
    }
}
