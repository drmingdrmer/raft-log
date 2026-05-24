use std::fmt;
use std::fs::File;
use std::sync::Arc;

use crate::ChunkId;
use crate::WalTypes;

pub(crate) struct ChunkPersisted {
    pub(crate) file: Arc<File>,
    pub(crate) starting_offset: u64,
    pub(crate) synced_offset: u64,
}

impl fmt::Debug for ChunkPersisted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChunkPersisted")
            .field("file", &self.file)
            .field("starting_offset", &ChunkId(self.starting_offset))
            .field("synced_offset", &ChunkId(self.synced_offset))
            .finish()
    }
}

/// Callback invoked after a chunk file is persisted.
///
/// The first argument describes the fsync event. The second argument carries
/// caller-defined state associated with the persisted chunk.
pub(crate) type ChunkPersistedFn<W> = Arc<
    dyn Fn(ChunkPersisted, Option<Arc<<W as WalTypes>::Checkpoint>>)
        + Send
        + Sync,
>;

pub(crate) struct ChunkPersistedCallback<W>
where W: WalTypes
{
    callback: ChunkPersistedFn<W>,
    prev_chunk_state: Option<Arc<W::Checkpoint>>,
}

impl<W> ChunkPersistedCallback<W>
where W: WalTypes
{
    pub(crate) fn new(
        callback: ChunkPersistedFn<W>,
        prev_chunk_state: Option<Arc<W::Checkpoint>>,
    ) -> Self {
        Self {
            callback,
            prev_chunk_state,
        }
    }
}

impl<W> ChunkPersistedCallback<W>
where W: WalTypes
{
    pub(crate) fn call(&self, persisted: ChunkPersisted) {
        (self.callback)(persisted, self.prev_chunk_state.clone());
    }
}
