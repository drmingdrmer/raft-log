use crate::WALRecord;
use crate::chunk::Chunk;
use crate::raft_log::stat::ChunkStat;

#[derive(Debug, Clone)]
pub(crate) struct ClosedChunk<A, C> {
    pub(crate) state: C,
    pub(crate) chunk: Chunk<WALRecord<A, C>>,
}

impl<A, C> ClosedChunk<A, C> {
    pub(crate) fn new(chunk: Chunk<WALRecord<A, C>>, state: C) -> Self {
        Self { state, chunk }
    }
}

impl<A, C> ClosedChunk<A, C>
where C: Clone
{
    pub(crate) fn stat(&self) -> ChunkStat<C> {
        ChunkStat {
            chunk_id: self.chunk.chunk_id(),
            records_count: self.chunk.records_count() as u64,
            global_start: self.chunk.global_start(),
            global_end: self.chunk.global_end(),
            size: self.chunk.chunk_size(),
            log_state: self.state.clone(),
        }
    }
}
