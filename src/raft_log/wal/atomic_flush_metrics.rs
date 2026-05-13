use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use crate::raft_log::stat::FlushMetrics;
use crate::raft_log::wal::batch_metrics::BatchMetrics;

#[derive(Debug, Default)]
pub(crate) struct AtomicFlushMetrics {
    batch_count: AtomicU64,
    sync_batch_count: AtomicU64,
    write_request_count: AtomicU64,
    write_bytes: AtomicU64,
    callback_count: AtomicU64,
    group_wait_count: AtomicU64,
    group_wait_us: AtomicU64,
    group_wait_max_us: AtomicU64,
    queued_wait_us: AtomicU64,
    queued_wait_max_us: AtomicU64,
    write_us: AtomicU64,
    write_max_us: AtomicU64,
    sync_us: AtomicU64,
    sync_max_us: AtomicU64,
    batch_us: AtomicU64,
    batch_max_us: AtomicU64,
    batch_size_max: AtomicU64,
    batch_bytes_max: AtomicU64,
    last_batch_size: AtomicU64,
    last_batch_bytes: AtomicU64,
    last_callback_count: AtomicU64,
    last_sync_us: AtomicU64,
    last_queued_wait_max_us: AtomicU64,
}

impl AtomicFlushMetrics {
    pub(crate) fn record_batch(&self, metrics: BatchMetrics) {
        self.batch_count.fetch_add(1, Ordering::Relaxed);
        if metrics.sync_batch {
            self.sync_batch_count.fetch_add(1, Ordering::Relaxed);
        }
        self.write_request_count
            .fetch_add(metrics.batch_size, Ordering::Relaxed);
        self.write_bytes.fetch_add(metrics.write_bytes, Ordering::Relaxed);
        self.callback_count
            .fetch_add(metrics.callback_count, Ordering::Relaxed);
        if metrics.group_wait_us > 0 {
            self.group_wait_count.fetch_add(1, Ordering::Relaxed);
            self.group_wait_us
                .fetch_add(metrics.group_wait_us, Ordering::Relaxed);
            update_max(&self.group_wait_max_us, metrics.group_wait_us);
        }
        self.queued_wait_us
            .fetch_add(metrics.queued_wait_us, Ordering::Relaxed);
        update_max(&self.queued_wait_max_us, metrics.queued_wait_max_us);
        self.write_us.fetch_add(metrics.write_us, Ordering::Relaxed);
        update_max(&self.write_max_us, metrics.write_us);
        if metrics.sync_us > 0 {
            self.sync_us.fetch_add(metrics.sync_us, Ordering::Relaxed);
            update_max(&self.sync_max_us, metrics.sync_us);
        }
        self.batch_us.fetch_add(metrics.batch_us, Ordering::Relaxed);
        update_max(&self.batch_max_us, metrics.batch_us);
        update_max(&self.batch_size_max, metrics.batch_size);
        update_max(&self.batch_bytes_max, metrics.write_bytes);
        self.last_batch_size.store(metrics.batch_size, Ordering::Relaxed);
        self.last_batch_bytes.store(metrics.write_bytes, Ordering::Relaxed);
        self.last_callback_count
            .store(metrics.callback_count, Ordering::Relaxed);
        self.last_sync_us.store(metrics.sync_us, Ordering::Relaxed);
        self.last_queued_wait_max_us
            .store(metrics.queued_wait_max_us, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self) -> FlushMetrics {
        FlushMetrics {
            batch_count: self.batch_count.load(Ordering::Relaxed),
            sync_batch_count: self.sync_batch_count.load(Ordering::Relaxed),
            write_request_count: self
                .write_request_count
                .load(Ordering::Relaxed),
            write_bytes: self.write_bytes.load(Ordering::Relaxed),
            callback_count: self.callback_count.load(Ordering::Relaxed),
            group_wait_count: self.group_wait_count.load(Ordering::Relaxed),
            group_wait_us: self.group_wait_us.load(Ordering::Relaxed),
            group_wait_max_us: self.group_wait_max_us.load(Ordering::Relaxed),
            queued_wait_us: self.queued_wait_us.load(Ordering::Relaxed),
            queued_wait_max_us: self.queued_wait_max_us.load(Ordering::Relaxed),
            write_us: self.write_us.load(Ordering::Relaxed),
            write_max_us: self.write_max_us.load(Ordering::Relaxed),
            sync_us: self.sync_us.load(Ordering::Relaxed),
            sync_max_us: self.sync_max_us.load(Ordering::Relaxed),
            batch_us: self.batch_us.load(Ordering::Relaxed),
            batch_max_us: self.batch_max_us.load(Ordering::Relaxed),
            batch_size_max: self.batch_size_max.load(Ordering::Relaxed),
            batch_bytes_max: self.batch_bytes_max.load(Ordering::Relaxed),
            last_batch_size: self.last_batch_size.load(Ordering::Relaxed),
            last_batch_bytes: self.last_batch_bytes.load(Ordering::Relaxed),
            last_callback_count: self
                .last_callback_count
                .load(Ordering::Relaxed),
            last_sync_us: self.last_sync_us.load(Ordering::Relaxed),
            last_queued_wait_max_us: self
                .last_queued_wait_max_us
                .load(Ordering::Relaxed),
        }
    }
}

fn update_max(current: &AtomicU64, value: u64) {
    let mut old = current.load(Ordering::Relaxed);
    while value > old {
        match current.compare_exchange_weak(
            old,
            value,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(next) => old = next,
        }
    }
}
