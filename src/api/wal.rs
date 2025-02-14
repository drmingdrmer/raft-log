//! Write-Ahead Log (WAL) module that provides interfaces for durable logging.
//!
//! The WAL is a critical component for maintaining data consistency and
//! durability in distributed systems. It ensures that all state changes are
//! first recorded persistently before being applied, enabling:
//!
//! - Crash recovery: System can recover its state by replaying the log
//! - Consistency: Ordered record of all state transitions
//! - Durability: Persistent storage of operations
//!
//! This module defines the core WAL trait that implementations must satisfy.

use std::io;

use crate::types::Segment;

/// Write-Ahead Log (WAL) trait that provides durability and consistency
/// guarantees for state machine operations.
///
/// The WAL ensures that all modifications are first recorded in a sequential
/// log before being applied to the state machine. This provides crash recovery
/// and helps maintain data consistency.
///
/// Type parameter `R` represents the record type that will be stored in the
/// log.
pub trait WAL<R> {
    // type StateMachine: StateMachine<R>;

    /// Appends a new record to the write-ahead log.
    ///
    /// This method provides durability by ensuring that records are written to
    /// persistent storage before returning.
    fn append(&mut self, rec: &R) -> Result<(), io::Error>;

    /// Returns the segment representing the last record in the write-ahead log.
    ///
    /// A segment contains the offset and size of a record in the log file.
    /// After appending a record, this method returns a segment describing the
    /// location and length of that record in the WAL.
    fn last_segment(&self) -> Segment;
}
