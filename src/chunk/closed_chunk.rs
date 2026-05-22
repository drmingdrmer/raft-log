use std::sync::Arc;

use crate::WALRecord;
use crate::chunk::Chunk;
use crate::raft_log::stat::ChunkStat;

#[derive(Debug, Clone)]
pub(crate) struct ClosedChunk<Act, Chkp> {
    pub(crate) state: Arc<Chkp>,
    pub(crate) chunk: Chunk<WALRecord<Act, Chkp>>,
}

impl<Act, Chkp> ClosedChunk<Act, Chkp> {
    pub(crate) fn new(
        chunk: Chunk<WALRecord<Act, Chkp>>,
        state: Arc<Chkp>,
    ) -> Self {
        Self { state, chunk }
    }
}

impl<Act, Chkp> ClosedChunk<Act, Chkp>
where Chkp: Clone
{
    pub(crate) fn stat(&self) -> ChunkStat<Chkp> {
        ChunkStat {
            chunk_id: self.chunk.chunk_id(),
            records_count: self.chunk.records_count() as u64,
            global_start: self.chunk.global_start(),
            global_end: self.chunk.global_end(),
            size: self.chunk.chunk_size(),
            log_state: self.state.as_ref().clone(),
        }
    }
}
