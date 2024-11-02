use std::io;
use std::sync::mpsc::SyncSender;

use codeq::Segment;

use crate::RaftLog;
use crate::Types;

pub trait RaftLogWriter<T: Types> {
    /// Update the Raft log state.
    ///
    /// This method is called when the Raft node needs to update its own state.
    fn save_user_data(
        &mut self,
        user_data: Option<T::UserData>,
    ) -> Result<Segment, io::Error>;

    /// Save the vote.
    ///
    /// This method is called when the Raft node receives a vote, such as,
    /// either term or `voted_for` is updated.
    ///
    /// This method does not flush immediately. The caller should call
    /// [`Self::flush`] to ensure the data is durably stored.
    fn save_vote(&mut self, vote: T::Vote) -> Result<Segment, io::Error>;

    /// Append a batch of entries to the log.
    ///
    /// This method is called when the Raft Leader receives a client write or
    /// the Follower receives an AppendEntries RPC.
    ///
    /// This method does not flush immediately. The caller should call
    /// [`Self::flush`] to ensure the data is durably stored.
    fn append<I>(&mut self, entries: I) -> Result<Segment, io::Error>
    where I: IntoIterator<Item = (T::LogId, T::LogPayload)>;

    /// Truncate the log entries at and after the given index.
    ///
    /// This method is called when the Raft Follower receives an AppendEntries
    /// RPC with a conflict index.
    ///
    /// This method does not flush immediately. The caller should call
    /// [`Self::flush`] to ensure the data is durably stored.
    fn truncate(&mut self, index: u64) -> Result<Segment, io::Error>;

    /// Purge the log entries before and at the given log id.
    ///
    /// This method is called when a Raft Node decide to remove logs that are
    /// already persisted in the state machine.
    ///
    /// The given log id is allowed to advance the `last` log id in the Raft
    /// log.
    ///
    /// This method does not flush immediately. The caller should call
    /// [`Self::flush`] to ensure the data is durably stored.
    fn purge(&mut self, upto: T::LogId) -> Result<Segment, io::Error>;

    /// Update the committed log id.
    ///
    /// This method is called when the Raft Leader updates the commit index or
    /// the Follower receives an AppendEntries RPC with a higher commit
    /// index.
    ///
    /// Usually, it is optional to persist the commit log id for a Raft
    /// implementation. But persisting the commit log id can help when
    /// re-applying the state machine when the Raft node restarts.
    ///
    /// This method does not flush immediately. The caller should call
    /// [`Self::flush`] to ensure the data is durably stored.
    fn commit(&mut self, log_id: T::LogId) -> Result<Segment, io::Error>;

    /// Request to flush all written data to persistent storage.
    ///
    /// This method initiates an asynchronous flush operation that ensures all
    /// data written up to the current offset is durably stored in
    /// persistent storage. The provided callback will be invoked once the
    /// flush operation completes.
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
