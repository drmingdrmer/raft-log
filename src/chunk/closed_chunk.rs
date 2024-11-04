use crate::chunk::Chunk;
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
}
