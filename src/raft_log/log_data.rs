use crate::types::Segment;

use crate::ChunkId;
use crate::Types;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogData<T: Types> {
    pub(crate) log_id: T::LogId,
    pub(crate) chunk_id: ChunkId,
    pub(crate) record_segment: Segment,
}

impl<T: Types> LogData<T> {
    pub(crate) fn new(
        log_id: T::LogId,
        chunk_id: ChunkId,
        record_segment: Segment,
    ) -> Self {
        Self {
            log_id,
            chunk_id,
            record_segment,
        }
    }
}
