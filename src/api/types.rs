//! Core type definitions for the Raft-log implementation.
//!
//! This module defines the `Types` trait which serves as a type system for the
//! entire Raft-log implementation. It allows users to customize core data types
//! such as log id, log payload, vote, callback, and user data.

use std::fmt::Debug;

use codeq::Codec;

use crate::raft_log::wal::callback::Callback;

/// The `Types` trait defines the core type parameters used throughout the
/// Raft-log implementation.
///
/// This trait provides an abstraction layer for the log implementation by
/// defining the core types used throughout the system. Implementations can
/// customize the concrete types for: log id, log payload, vote, callback, and
/// user data.
pub trait Types
where Self: Debug + Default + PartialEq + Eq + Clone + 'static
{
    /// Unique identifier for log entries. Usually it contains the term and log
    /// index.
    type LogId: Debug + Clone + Ord + Eq + Codec + Send + Sync + 'static;

    /// The actual data/command stored in log entries.
    type LogPayload: Debug + Clone + Codec + Send + Sync + 'static;

    /// Representation of vote in leader election.
    ///
    /// A vote typically contains:
    /// - The election term number
    /// - The candidate node ID
    /// - A commitment flag indicating whether a quorum of nodes has granted
    ///   this vote
    ///
    /// The commitment flag is set to true when a majority of nodes in the
    /// cluster have voted for this candidate in this term.
    ///
    /// In some raft implementation the vote is called `hard state`
    type Vote: Debug + Clone + PartialOrd + Eq + Codec + 'static;

    /// Callback handlers for notification of an IO operation.
    type Callback: Callback + Send + 'static;

    /// Custom data that can be attached to Raft-log.
    ///
    /// This data is not used by the Raft-log implementation, but it can be used
    /// by the user. For example, an application could attach a
    /// configuration or the node info in this field.
    type UserData: Debug + Clone + Eq + Codec + 'static;

    /// Get the log index from the log id.
    fn log_index(log_id: &Self::LogId) -> u64;

    /// Get the payload size from the log payload.
    ///
    /// The returned size does not have to be accurate. It is only used to
    /// estimate the space occupied in the cache.
    fn payload_size(payload: &Self::LogPayload) -> u64;

    /// Get the next log index from the log id.
    fn next_log_index(log_id: Option<&Self::LogId>) -> u64 {
        match log_id {
            Some(log_id) => Self::log_index(log_id) + 1,
            None => 0,
        }
    }
}
