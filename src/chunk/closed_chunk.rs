use crate::chunk::Chunk;
use crate::raft_log::stat::ChunkStat;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;
use crate::Types;

#[derive(Debug, Clone)]
pub(crate) struct ClosedChunk<T>
where T: Types
{
    pub(crate) state: RaftLogState<T>,
    pub(crate) chunk: Chunk<T>,
}

impl<T> ClosedChunk<T>
where T: Types
{
    pub(crate) fn new(chunk: Chunk<T>, state: RaftLogState<T>) -> Self {
        Self { state, chunk }
    }

    pub(crate) fn stat(&self) -> ChunkStat<T> {
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
