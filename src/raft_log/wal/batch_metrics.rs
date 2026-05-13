use std::time::Duration;
use std::time::Instant;

use crate::Types;
use crate::raft_log::wal::queued_write::QueuedWrite;

#[derive(Debug, Default)]
pub(crate) struct BatchMetrics {
    pub(crate) batch_size: u64,
    pub(crate) sync_batch: bool,
    pub(crate) write_bytes: u64,
    pub(crate) callback_count: u64,
    pub(crate) group_wait_us: u64,
    pub(crate) queued_wait_us: u64,
    pub(crate) queued_wait_max_us: u64,
    pub(crate) write_us: u64,
    pub(crate) sync_us: u64,
    pub(crate) batch_us: u64,
}

impl BatchMetrics {
    pub(crate) fn new(batch_size: usize, group_wait: Duration) -> Self {
        Self {
            batch_size: batch_size as u64,
            sync_batch: false,
            group_wait_us: duration_micros(group_wait),
            ..Default::default()
        }
    }

    pub(crate) fn record_queued_write<T: Types>(
        &mut self,
        batch_start: Instant,
        w: &QueuedWrite<T>,
    ) {
        let wait_us = duration_micros(batch_start.duration_since(w.queued_at));
        self.queued_wait_us += wait_us;
        self.queued_wait_max_us = self.queued_wait_max_us.max(wait_us);
        self.write_bytes += w.write.data.len() as u64;
        if w.write.callback.is_some() {
            self.callback_count += 1;
        }
    }

    pub(crate) fn record_write_time(&mut self, write_start: Instant) {
        self.write_us = duration_micros(write_start.elapsed());
    }

    pub(crate) fn record_sync_time(&mut self, sync_start: Instant) {
        self.sync_batch = true;
        self.sync_us = duration_micros(sync_start.elapsed());
    }

    pub(crate) fn record_batch_time(&mut self, batch_start: Instant) {
        self.batch_us = duration_micros(batch_start.elapsed());
    }
}

fn duration_micros(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}
