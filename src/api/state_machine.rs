//! State machine abstraction for applying records in a deterministic order.

use std::fmt::Debug;

use crate::ChunkId;
use crate::WALRecord;
use crate::types::Segment;

/// A trait representing a state machine of [`WAL`] that can apply records to
/// modify its state.
///
/// The Raft-log follows a Write-Ahead Log (WAL) + State Machine pattern. This
/// trait defines the state machine component that processes records persisted
/// in the WAL to build and maintain application state.
///
/// # Type Parameters
/// * `A` - The type of actions that can be applied to the state machine
///
/// [`WAL`]: crate::api::wal::WAL
pub trait StateMachine<A> {
    /// The type of error that can occur during record application
    type Error: std::error::Error + Debug + 'static;

    /// The compact state-machine data persisted by the WAL at chunk
    /// boundaries.
    ///
    /// This value is intentionally opaque to the WAL. It is not necessarily the
    /// complete state-machine state; it is the small part of the state machine
    /// that should survive restart even when the full state machine is stored
    /// elsewhere or reconstructed by replay.
    type Checkpoint;

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
        record: &WALRecord<A, Self::Checkpoint>,
        chunk_id: ChunkId,
        global_segment: Segment,
    ) -> Result<(), Self::Error>;

    /// Returns the current checkpoint value for WAL storage.
    ///
    /// The WAL stores this value at the beginning of each new chunk. Keep it
    /// small because it may be duplicated across chunks. The framework only
    /// persists the value; the semantic content belongs to the state-machine
    /// implementation.
    fn checkpoint(&self) -> Self::Checkpoint;
}
