use std::io;
use std::sync::mpsc::SyncSender;

/// A trait for handling IO operation callbacks.
///
/// This trait defines a mechanism to send IO operation results back to the
/// caller, typically used for asynchronous IO operations.
pub trait Callback {
    /// Sends the result of a IO operation back to the caller.
    ///
    /// # Arguments
    ///
    /// * `res` - The result of the operation, either success (Ok(())) or an IO
    ///   error
    fn send(self, res: Result<(), io::Error>);
}

/// Implementation of the Callback trait for SyncSender.
///
/// This allows using a synchronous channel sender as a callback mechanism
/// for IO operations.
impl Callback for SyncSender<Result<(), io::Error>> {
    fn send(self, res: Result<(), io::Error>) {
        let _ = SyncSender::send(&self, res);
    }
}
