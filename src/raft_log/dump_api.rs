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
        let write_record = |chunk_id, in_chunk_record_index, res| {
            dump_writer::write_record_debug(
                &mut w,
                chunk_id,
                in_chunk_record_index,
                res,
            )
        };
        self.write_with(write_record)
    }

    /// Writes the Raft log contents to the provided writer, using
    /// `std::fmt::Display` for record formatting.
    fn write_display<W: io::Write>(&self, mut w: W) -> Result<(), io::Error>
    where
        WALRecord<T>: std::fmt::Display,
    {
        writeln!(&mut w, "RaftLog:")?;
        let write_record = |chunk_id, in_chunk_record_index, res| {
            dump_writer::write_record_display(
                &mut w,
                chunk_id,
                in_chunk_record_index,
                res,
            )
        };
        self.write_with(write_record)
    }

    /// Writes the Raft log contents using a custom record writer function.
    ///
    /// # Arguments
    /// * `write_record` - A function that writes individual log records. It
    ///   takes:
    ///   - `ChunkId`: The ID of the chunk containing the record
    ///   - `u64`: The index of the record in its chunk
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

#[cfg(test)]
mod tests {
    use std::io;
    use std::io::Write;

    use super::*;
    use crate::testing::TestDisplayTypes;
    use crate::types::Segment;

    struct MockDump {
        seg: Segment,
        record: WALRecord<TestDisplayTypes>,
    }

    impl DumpApi<TestDisplayTypes> for MockDump {
        fn write_with<D>(&self, mut write_record: D) -> Result<(), io::Error>
        where
            D: FnMut(ChunkId, u64, Result<(Segment, WALRecord<TestDisplayTypes>), io::Error>) -> Result<(), io::Error>,
        {
            write_record(ChunkId(0), 0, Ok((self.seg, self.record.clone())))?;
            Ok(())
        }
    }

    #[test]
    fn test_write_to_string() -> Result<(), io::Error> {
        let dump = MockDump {
            seg: Segment::new(0, 10),
            record: WALRecord::SaveVote(1),
        };

        let got = dump.write_to_string()?;
        let want = "RaftLog:\nChunkId(00_000_000_000_000_000_000)\n  R-00000: [000_000_000, 000_000_010) Size(10): SaveVote(1)\n";
        assert_eq!(want, got);
        Ok(())
    }

    #[test]
    fn test_write_uses_debug_format() -> Result<(), io::Error> {
        let dump = MockDump {
            seg: Segment::new(0, 10),
            record: WALRecord::Append(3, "hello".to_string()),
        };

        let mut buf = Vec::new();
        dump.write(&mut buf)?;
        let got = String::from_utf8(buf).unwrap();

        let want = "RaftLog:\nChunkId(00_000_000_000_000_000_000)\n  R-00000: [000_000_000, 000_000_010) Size(10): Append(3, \"hello\")\n";
        assert_eq!(want, got);
        Ok(())
    }

    #[test]
    fn test_write_display_uses_display_format() -> Result<(), io::Error> {
        let dump = MockDump {
            seg: Segment::new(0, 10),
            record: WALRecord::Append(3, "hello".to_string()),
        };

        let mut buf = Vec::new();
        dump.write_display(&mut buf)?;
        let got = String::from_utf8(buf).unwrap();

        let want = "RaftLog:\nChunkId(00_000_000_000_000_000_000)\n  R-00000: [000_000_000, 000_000_010) Size(10): Append(log_id: 3, payload: hello)\n";
        assert_eq!(want, got);
        Ok(())
    }

    #[test]
    fn test_write_with_custom_writer() -> Result<(), io::Error> {
        let dump = MockDump {
            seg: Segment::new(0, 10),
            record: WALRecord::Commit(5),
        };

        let mut custom_output = Vec::new();
        dump.write_with(|chunk_id, index, res| {
            if let Ok((_, rec)) = res {
                writeln!(&mut custom_output, "custom: {} {} {:?}", chunk_id.0, index, rec)?;
            }
            Ok(())
        })?;

        let output = String::from_utf8(custom_output).unwrap();
        assert_eq!("custom: 0 0 Commit(5)\n", output);
        Ok(())
    }
}
