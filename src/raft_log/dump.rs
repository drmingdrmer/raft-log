use std::io;
use std::io::Error;
use std::sync::Arc;

use chunked_wal::Chunk;
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
    pub(crate) config: Arc<Config>,
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
    fn write_with<D>(&self, mut write_record: D) -> Result<(), Error>
    where D: FnMut(
            ChunkId,
            u64,
            Result<(Segment, RaftLogRecord<T>), Error>,
        ) -> Result<(), Error> {
        let closed =
            self.raft_log.wal.closed.values().map(|c| c.chunk.chunk_id());

        let chunk_ids = closed.chain([self.raft_log.wal.open.chunk.chunk_id()]);

        for chunk_id in chunk_ids {
            let f = Chunk::<RaftLogRecord<T>>::open_chunk_file(
                &self.config.wal,
                chunk_id,
            )?;

            let it = Chunk::<RaftLogRecord<T>>::load_records_iter(
                &self.config.wal,
                Arc::new(f),
                chunk_id,
            )?;

            for (i, res) in it.enumerate() {
                write_record(chunk_id, i as u64, res)?;
            }
        }

        Ok(())
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
