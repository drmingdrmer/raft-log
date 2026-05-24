//! Associated types used by the generic WAL implementation.

use std::fmt::Debug;

use codeq::Codec;

use crate::Callback;

/// Defines the concrete types used by the WAL.
pub trait WalTypes
where Self: Debug + Default + PartialEq + Eq + Clone + 'static
{
    type Action: Debug + Clone + Codec + Send + 'static;

    type Checkpoint: Debug
        + Clone
        + PartialEq
        + Eq
        + Codec
        + Send
        + Sync
        + 'static;

    /// Callback handlers for notification of an IO operation.
    type Callback: Callback;
}
