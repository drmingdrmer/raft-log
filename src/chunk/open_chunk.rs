use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::sync::Arc;

use codeq::Encode;

use crate::ChunkId;
use crate::Config;
use crate::chunk::Chunk;
use crate::types::Segment;

#[derive(Debug)]
pub(crate) struct OpenChunk<R> {
    pending_data: Vec<u8>,
    pub(crate) chunk: Chunk<R>,
}

impl<R> OpenChunk<R> {
    /// Creates a new open chunk from an existing chunk.
    pub(crate) fn new(chunk: Chunk<R>) -> Self {
        Self {
            pending_data: Vec::new(),
            chunk,
        }
    }

    pub(crate) fn take_pending_data(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending_data)
    }
}

impl<R> OpenChunk<R>
where R: Encode
{
    pub(crate) fn create(
        config: Arc<Config>,
        chunk_id: ChunkId,
        initial_record: R,
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
            truncated: None,
            _p: Default::default(),
        };

        let mut open = Self {
            pending_data: Vec::new(),
            chunk,
        };

        open.append_record(&initial_record)?;
        open.chunk.f.write_all(&open.pending_data)?;
        open.pending_data.clear();

        Ok(open)
    }

    pub(crate) fn append_record(
        &mut self,
        rec: &R,
    ) -> Result<Segment, io::Error> {
        let size = rec.encode(&mut self.pending_data)?;

        self.chunk.append_record_size(size as u64);

        Ok(self.chunk.last_segment())
    }
}
