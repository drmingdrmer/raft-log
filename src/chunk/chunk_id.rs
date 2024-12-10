use std::fmt;
use std::ops::Deref;

use crate::num::format_pad_u64;

/// ChunkId represents a unique identifier for a chunk based on its global
/// offset in the log.
///
/// Each chunk in the log has a unique position identified by its starting
/// offset. This offset serves as the chunk's identifier and can be used to
/// locate and reference specific chunks within the log.
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
    /// Returns the global offset where this chunk begins in the log.
    ///
    /// The offset is a monotonically increasing value that represents the
    /// absolute position of the chunk in the entire log sequence.
    pub fn offset(&self) -> u64 {
        self.0
    }
}
