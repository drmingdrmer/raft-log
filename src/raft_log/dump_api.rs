use std::io;

use codeq::Segment;

use crate::dump_writer;
use crate::ChunkId;
use crate::Types;
use crate::WALRecord;

pub trait DumpApi<T: Types> {
    fn write_to_string(&self) -> Result<String, io::Error> {
        let mut buf = Vec::new();
        self.write(&mut buf)?;
        String::from_utf8(buf)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    fn write<W: io::Write>(&self, mut w: W) -> Result<(), io::Error> {
        writeln!(&mut w, "RaftLog:")?;
        let write_line = |chunk_id, i, res| {
            dump_writer::multiline_string(&mut w, chunk_id, i, res)
        };
        self.write_with(write_line)
    }

    fn write_with<D>(&self, write_record: D) -> Result<(), io::Error>
    where D: FnMut(
            ChunkId,
            u64,
            Result<(Segment, WALRecord<T>), io::Error>,
        ) -> Result<(), io::Error>;
}
