use std::fmt;
use std::ops::Deref;

use crate::num::format_pad_u64;

/// ChunkId is defined with the global offset of the chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkId(pub u64);

impl fmt::Display for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChunkId({})", format_pad_u64(self.0))
    }
}

impl From<u64> for ChunkId {
    fn from(offset: u64) -> Self {
        ChunkId(offset)
    }
}

impl Deref for ChunkId {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ChunkId {
    /// Return the global offset this chunk starts at.
    pub fn offset(&self) -> u64 {
        self.0
    }
}
