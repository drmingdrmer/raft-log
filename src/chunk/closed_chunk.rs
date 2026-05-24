use std::sync::Arc;

use crate::WALRecord;
use crate::WalTypes;
use crate::chunk::Chunk;
use crate::raft_log::stat::ChunkStat;

#[derive(Debug, Clone)]
pub(crate) struct ClosedChunk<W>
where W: WalTypes
{
    pub(crate) state: Arc<W::Checkpoint>,
    pub(crate) chunk: Chunk<WALRecord<W>>,
}

impl<W> ClosedChunk<W>
where W: WalTypes
{
    pub(crate) fn new(
        chunk: Chunk<WALRecord<W>>,
        state: Arc<W::Checkpoint>,
    ) -> Self {
        Self { state, chunk }
    }
}

impl<W> ClosedChunk<W>
where W: WalTypes
{
    pub(crate) fn stat(&self) -> ChunkStat<W::Checkpoint> {
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
