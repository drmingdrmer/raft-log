//! Define the writing interface for the Raft-log.
//!
//! This module provides the interface for durably storing Raft state to
//! persistent storage. The RaftLogWriter trait defines the core operations
//! needed to maintain a persistent Raft log, including:
//! - Log entry management (append, truncate, purge)
//! - Vote and term information
//! - Commit index tracking
//! - User state persistence
//!
//! Implementations of this trait must ensure that all operations are atomic and
//! durable when flushed, as required by the Raft protocol for correctness
//! during node failures and restarts.

use std::io;
use std::sync::mpsc::SyncSender;

use codeq::Segment;

use crate::RaftLog;
use crate::Types;

/// Define the writing interface for the Raft-log.
///
/// The Raft-log writer is responsible for writing data to the log file. It
/// provides methods required by a Raft-log implementation for durably storing
/// Raft state, including log entries, votes, user data, and commit information.
///
/// Note: Most methods in this trait do not flush immediately for performance
/// reasons. Call [`Self::flush`] explicitly to ensure data is durably
/// persisted.
pub trait RaftLogWriter<T: Types> {
    /// Update the user data of the Raft log.
    ///
    /// This method is called when the Raft node needs to update its
    /// application-specific state. The user data is optional and can be
    /// None to clear existing data.
    ///
    /// Returns a Segment representing the written data region.
    fn save_user_data(
        &mut self,
        user_data: Option<T::UserData>,
    ) -> Result<Segment, io::Error>;

    /// Save the vote information.
    ///
    /// This method is called when:
    /// - The node's term is updated
    /// - The node votes for a candidate
    /// - The node's vote state changes
    ///
    /// Note: Does not flush immediately. Call [`Self::flush`] to ensure
    /// durability.
    ///
    /// Returns a Segment representing the written data region.
    fn save_vote(&mut self, vote: T::Vote) -> Result<Segment, io::Error>;

    /// Append a batch of entries to the log.
    ///
    /// This method is called when:
    /// - The leader receives client write requests
    /// - A follower receives AppendEntries RPC from the leader
    ///
    /// Note: Does not flush immediately. Call [`Self::flush`] to ensure
    /// durability.
    ///
    /// Returns a Segment representing the written data region.
    fn append<I>(&mut self, entries: I) -> Result<Segment, io::Error>
    where I: IntoIterator<Item = (T::LogId, T::LogPayload)>;

    /// Truncate the log by removing entries at and after the given index.
    ///
    /// This method is called when a follower discovers conflicting entries
    /// during an AppendEntries RPC. The follower must remove all
    /// conflicting entries before appending new ones from the leader.
    ///
    /// Note: Does not flush immediately. Call [`Self::flush`] to ensure
    /// durability.
    ///
    /// Returns a Segment representing the modified data region.
    fn truncate(&mut self, index: u64) -> Result<Segment, io::Error>;

    /// Purge log entries up to and including the given log id.
    ///
    /// This method is called to remove logs that have been successfully applied
    /// to the state machine and are no longer needed. This helps manage
    /// storage space and improve performance.
    ///
    /// The given log id can advance the last log id in the Raft log,
    /// effectively moving the log's starting point forward.
    ///
    /// Note: Does not flush immediately. Call [`Self::flush`] to ensure
    /// durability.
    ///
    /// Returns a Segment representing the modified data region.
    fn purge(&mut self, upto: T::LogId) -> Result<Segment, io::Error>;

    /// Update the committed log id.
    ///
    /// This method is called when:
    /// - The leader advances its commit index
    /// - A follower learns of a higher commit index from AppendEntries RPC
    ///
    /// While persisting the commit index is optional in Raft, it can optimize
    /// state machine replay during node restart by avoiding re-application of
    /// already committed entries.
    ///
    /// Note: Does not flush immediately. Call [`Self::flush`] to ensure
    /// durability.
    ///
    /// Returns a Segment representing the written data region.
    fn commit(&mut self, log_id: T::LogId) -> Result<Segment, io::Error>;

    /// Initiate an asynchronous flush operation to persist all written data.
    ///
    /// This method ensures all previously written data is durably stored on
    /// disk. The provided callback will be invoked when the flush operation
    /// completes, indicating success or failure.
    ///
    /// This should be called after any operation that requires immediate
    /// durability, such as responding to client requests or voting in
    /// elections.
    fn flush(&mut self, callback: T::Callback) -> Result<(), io::Error>;
}

/// Synchronously flush all written data to persistent storage.
#[allow(dead_code)]
pub(crate) fn blocking_flush<T>(rl: &mut RaftLog<T>) -> Result<(), io::Error>
where T: Types<Callback = SyncSender<Result<(), io::Error>>> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    rl.flush(tx)?;
    rx.recv().map_err(|_e| {
        io::Error::new(
            io::ErrorKind::Other,
            "Failed to receive flush completion",
        )
    })??;
    Ok(())
}
