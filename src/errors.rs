use std::io;

pub use chunked_wal::errors::InvalidChunkFileName;

use crate::api::types::Types;

#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(thiserror::Error)]
pub enum RaftLogStateError<T: Types> {
    #[error(transparent)]
    VoteReversal(#[from] VoteReversal<T>),

    #[error(transparent)]
    LogIdReversal(#[from] LogIdReversal<T>),

    #[error(transparent)]
    LogIdNonConsecutive(#[from] LogIdNonConsecutive<T>),

    #[error(transparent)]
    LogIdIndexDisorder(#[from] LogIdIndexDisorder<T>),

    #[error(transparent)]
    CheckpointLogMismatch(#[from] CheckpointLogMismatch<T>),

    #[error(transparent)]
    LogIndexNotFound(#[from] LogIndexNotFound),
}

impl<T: Types> From<RaftLogStateError<T>> for io::Error {
    fn from(value: RaftLogStateError<T>) -> Self {
        io::Error::new(io::ErrorKind::InvalidInput, value.to_string())
    }
}

/// Error indicating that a vote cannot be reversed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(thiserror::Error)]
#[error(
    "Vote cannot be reversed: current {current:?}, attempted {attempted:?}"
)]
pub struct VoteReversal<T: Types> {
    pub current: T::Vote,
    pub attempted: T::Vote,
}

impl<T: Types> VoteReversal<T> {
    pub fn new(current: T::Vote, attempted: T::Vote) -> Self {
        Self { current, attempted }
    }
}

/// Error indicating that a log id cannot be reversed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(thiserror::Error)]
#[error(
    "Log id cannot be reversed when {when}: current {current:?}, attempted {attempted:?}"
)]
pub struct LogIdReversal<T: Types> {
    pub current: T::LogId,
    pub attempted: T::LogId,
    pub when: &'static str,
}

impl<T: Types> LogIdReversal<T> {
    pub fn new(
        current: T::LogId,
        attempted: T::LogId,
        when: &'static str,
    ) -> Self {
        Self {
            current,
            attempted,
            when,
        }
    }
}

/// Error indicating that a log id is not consecutive to the last know one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(thiserror::Error)]
#[error(
    "Log id is not consecutive when append: last {last:?}, attempted {attempted:?}"
)]
pub struct LogIdNonConsecutive<T: Types> {
    pub last: Option<T::LogId>,
    pub attempted: T::LogId,
}

impl<T: Types> LogIdNonConsecutive<T> {
    pub fn new(last: Option<T::LogId>, attempted: T::LogId) -> Self {
        Self { last, attempted }
    }
}

/// Error indicating that a log id disagrees with the log id already stored at
/// the same or a nearby index.
///
/// A Raft log stores a greater log id at a greater index, so log id order and
/// index order must always agree. A pair that orders one way by log id and the
/// other way by index describes a log that was never written, which means the
/// caller and this log disagree about history.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(thiserror::Error)]
#[error(
    "Log id conflicts with the stored log id when {when}: stored {stored:?}, attempted {attempted:?}; log id order and log index order must agree"
)]
pub struct LogIdIndexDisorder<T: Types> {
    pub stored: T::LogId,
    pub attempted: T::LogId,
    pub when: &'static str,
}

impl<T: Types> LogIdIndexDisorder<T> {
    pub fn new(
        stored: T::LogId,
        attempted: T::LogId,
        when: &'static str,
    ) -> Self {
        Self {
            stored,
            attempted,
            when,
        }
    }
}

/// Error indicating that a checkpoint declares a log range other than the one
/// the store holds.
///
/// A checkpoint states that the log is exactly the entries in
/// `(purged, last]`. Storing a checkpoint that says otherwise would leave
/// reads serving entries the state calls purged, or hiding entries the state
/// calls present.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(thiserror::Error)]
#[error(
    "Checkpoint does not match the stored log: the log holds {first:?}..={last:?}, the checkpoint declares purged {purged:?} and last {declared_last:?}"
)]
pub struct CheckpointLogMismatch<T: Types> {
    pub first: T::LogId,
    pub last: T::LogId,
    pub purged: Option<T::LogId>,
    pub declared_last: Option<T::LogId>,
}

impl<T: Types> CheckpointLogMismatch<T> {
    pub fn new(
        first: T::LogId,
        last: T::LogId,
        purged: Option<T::LogId>,
        declared_last: Option<T::LogId>,
    ) -> Self {
        Self {
            first,
            last,
            purged,
            declared_last,
        }
    }
}

/// Error indicating that a log index is not found.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(thiserror::Error)]
#[error("Log not found at index {index:?}")]
pub struct LogIndexNotFound {
    pub index: u64,
}

impl LogIndexNotFound {
    pub fn new(index: u64) -> Self {
        Self { index }
    }
}
