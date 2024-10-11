mod storage_errors;

use std::io;

pub use storage_errors::InvalidChunkFileName;

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
#[error("Log id cannot be reversed when {when}: current {current:?}, attempted {attempted:?}")]
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
#[error("Log id is not consecutive when append: last {last:?}, attempted {attempted:?}")]
pub struct LogIdNonConsecutive<T: Types> {
    pub last: Option<T::LogId>,
    pub attempted: T::LogId,
}

impl<T: Types> LogIdNonConsecutive<T> {
    pub fn new(last: Option<T::LogId>, attempted: T::LogId) -> Self {
        Self { last, attempted }
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
