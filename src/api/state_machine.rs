//! State machine abstraction for applying records in a deterministic order.

use std::fmt::Debug;

use crate::ChunkId;
use crate::types::Segment;

/// A trait representing a state machine of [`WAL`] that can apply records to
/// modify its state.
///
/// The Raft-log follows a Write-Ahead Log (WAL) + State Machine pattern. This
/// trait defines the state machine component that processes records persisted
/// in the WAL to build and maintain application state.
///
/// # Type Parameters
/// * `R` - The type of records that can be applied to the state machine
///
/// [`WAL`]: crate::api::wal::WAL
pub trait StateMachine<R> {
    /// The type of error that can occur during record application
    type Error: std::error::Error + Debug + 'static;

    /// Applies a record that is already persisted in the WAL to the state
    /// machine, potentially modifying its state.
    ///
    /// # Arguments
    /// * `record` - The record to apply.
    /// * `chunk_id` - The identifier of the chunk containing this record.
    /// * `global_segment` - The global offset and size of the record in the log
    ///   file.
    fn apply(
        &mut self,
        record: &R,
        chunk_id: ChunkId,
        global_segment: Segment,
    ) -> Result<(), Self::Error>;
}
