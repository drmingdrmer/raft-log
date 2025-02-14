use std::io;

use crate::dump_writer;
use crate::types::Segment;
use crate::ChunkId;
use crate::Types;
use crate::WALRecord;

/// A trait for dumping Raft log contents in a human-readable format.
pub trait DumpApi<T: Types> {
    /// Writes the Raft log contents to a String.
    fn write_to_string(&self) -> Result<String, io::Error> {
        let mut buf = Vec::new();
        self.write(&mut buf)?;
        String::from_utf8(buf)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    /// Writes the Raft log contents to the provided writer.
    fn write<W: io::Write>(&self, mut w: W) -> Result<(), io::Error> {
        writeln!(&mut w, "RaftLog:")?;
        let write_line = |chunk_id, i, res| {
            dump_writer::multiline_string(&mut w, chunk_id, i, res)
        };
        self.write_with(write_line)
    }

    /// Writes the Raft log contents using a custom record writer function.
    ///
    /// # Arguments
    /// * `write_record` - A function that writes individual log records. It
    ///   takes:
    ///   - `ChunkId`: The ID of the chunk containing the record
    ///   - `u64`: The index of the record
    ///   - `Result<(Segment, WALRecord<T>), io::Error>`: The record data or
    ///     error
    ///
    /// # Returns
    /// - `Ok(())` if writing succeeds
    /// - `Err(io::Error)` if fails to read Raft-log or user provided callback
    ///   returns an error
    fn write_with<D>(&self, write_record: D) -> Result<(), io::Error>
    where D: FnMut(
            ChunkId,
            u64,
            Result<(Segment, WALRecord<T>), io::Error>,
        ) -> Result<(), io::Error>;
}
