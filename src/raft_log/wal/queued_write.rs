use std::time::Instant;

use crate::Types;
use crate::raft_log::wal::flush_request::WriteRequest;

pub(crate) struct QueuedWrite<T: Types> {
    pub(crate) queued_at: Instant,
    pub(crate) write: WriteRequest<T>,
}
