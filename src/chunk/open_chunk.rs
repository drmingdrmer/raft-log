use std::fs::OpenOptions;
use std::io;
use std::io::Seek;
use std::io::Write;
use std::sync::Arc;

use codeq::Encode;
use codeq::Segment;

use crate::chunk::Chunk;
use crate::ChunkId;
use crate::Config;
use crate::Types;
use crate::WALRecord;

#[derive(Debug)]
pub(crate) struct OpenChunk<T: Types> {
    record_write_buf: Vec<u8>,
    pub(crate) chunk: Chunk<T>,
}

impl<T> OpenChunk<T>
where T: Types
{
    pub(crate) fn create(
        config: Arc<Config>,
        chunk_id: ChunkId,
        initial_record: WALRecord<T>,
    ) -> Result<Self, io::Error> {
        let path = config.chunk_path(chunk_id);
        let f = OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(path)?;

        let record_offsets = vec![*chunk_id];

        let chunk = Chunk {
            f: Arc::new(f),
            global_offsets: record_offsets,
            _p: Default::default(),
        };

        let mut open = Self {
            record_write_buf: Vec::new(),
            chunk,
        };

        open.append_record(&initial_record)?;

        Ok(open)
    }

    pub(crate) fn append_record(
        &mut self,
        rec: &WALRecord<T>,
    ) -> Result<Segment, io::Error> {
        self.record_write_buf.clear();
        let size = rec.encode(&mut self.record_write_buf)?;

        let start = self.chunk.end_offset();
        self.chunk.f.seek(io::SeekFrom::Start(start))?;
        self.chunk.f.write_all(&self.record_write_buf)?;

        self.chunk.append_record_size(size as u64);

        Ok(self.chunk.last_segment())
    }
}
