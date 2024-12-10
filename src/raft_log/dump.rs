use std::io;
use std::io::Error;
use std::sync::Arc;

use codeq::Segment;

use crate::chunk::Chunk;
use crate::file_lock;
use crate::raft_log::dump_api::DumpApi;
use crate::ChunkId;
use crate::Config;
use crate::RaftLog;
use crate::Types;
use crate::WALRecord;

/// A dump utility that reads WAL records from disk.
///
/// It acquires an exclusive lock on the directory to prevent concurrent writes
/// while reading.
pub struct Dump<T> {
    config: Arc<Config>,

    /// Acquire the dir exclusive lock when writing to the log.
    _dir_lock: file_lock::FileLock,

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
    fn write_with<D>(&self, mut write_record: D) -> Result<(), io::Error>
    where D: FnMut(
            ChunkId,
            u64,
            Result<(Segment, WALRecord<T>), io::Error>,
        ) -> Result<(), io::Error> {
        let config = self.config.as_ref();

        let chunk_ids = RaftLog::<T>::load_chunk_ids(config)?;
        for chunk_id in chunk_ids {
            let it = Chunk::<T>::dump(config, chunk_id)?;
            for (i, res) in it.into_iter().enumerate() {
                write_record(chunk_id, i as u64, res)?;
            }
        }
        Ok(())
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
            Result<(Segment, WALRecord<T>), Error>,
        ) -> Result<(), Error> {
        let closed =
            self.raft_log.wal.closed.values().map(|c| c.chunk.chunk_id());

        let chunk_ids = closed.chain([self.raft_log.wal.open.chunk.chunk_id()]);

        for chunk_id in chunk_ids {
            let f =
                Chunk::<T>::open_chunk_file(self.config.as_ref(), chunk_id)?;

            let it = Chunk::load_records_iter(
                self.config.as_ref(),
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
        let dir_lock = file_lock::FileLock::new(config.clone())?;

        Ok(Self {
            config,
            _dir_lock: dir_lock,
            _p: std::marker::PhantomData,
        })
    }
}
