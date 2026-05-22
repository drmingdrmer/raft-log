use std::fmt;
use std::fmt::Formatter;

use display_more::DisplayDebugOptionExt;

use crate::ChunkId;
use crate::Types;
use crate::num::format_pad9_u64;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;

/// Statistics about a Raft log, including information about closed chunks, open
/// chunk, and payload cache performance metrics.
#[derive(Debug, Clone)]
pub struct Stat<T>
where T: Types
{
    /// Vector of statistics for closed (completed) chunks
    pub closed_chunks: Vec<ChunkStat<RaftLogState<T>>>,
    /// Statistics for the currently open (active) chunk
    pub open_chunk: ChunkStat<RaftLogState<T>>,
    /// The last evictable log id in the payload cache
    pub payload_cache_last_evictable: Option<T::LogId>,
    /// Current number of items in the payload cache
    pub payload_cache_item_count: u64,
    /// Maximum number of items allowed in the payload cache
    pub payload_cache_max_item: u64,
    /// Current size of the payload cache in bytes
    pub payload_cache_size: u64,
    /// Maximum capacity of the payload cache in bytes
    pub payload_cache_capacity: u64,
    /// Number of cache misses
    pub payload_cache_miss: u64,
    /// Number of cache hits
    pub payload_cache_hit: u64,
    /// Flush worker batching and IO metrics.
    pub flush_metrics: FlushMetrics,
}

impl<T> fmt::Display for Stat<T>
where T: Types
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let lb = if f.alternate() { "\n" } else { "" };
        let idt = if f.alternate() { "  " } else { "" };
        write!(
            f,
            "Stat{{{lb} closed_chunks: [{lb}{idt}{}{lb} ],{lb} open_chunk: {},{lb} payload_cache:{{\
            evictable: ..={},\
            item/max: {} / {},\
            size/cap: {} / {},\
            miss: {},\
            hit: {}\
            }}{lb}\
            }}",
            self.closed_chunks
                .iter()
                .map(|c| format!("{}", c))
                .collect::<Vec<String>>()
                .join(&format!(",{lb}{idt}")),
            self.open_chunk,
            self.payload_cache_last_evictable.display_debug(),
            format_pad9_u64(self.payload_cache_item_count),
            format_pad9_u64(self.payload_cache_max_item),
            format_pad9_u64(self.payload_cache_size),
            format_pad9_u64(self.payload_cache_capacity),
            format_pad9_u64(self.payload_cache_miss),
            format_pad9_u64(self.payload_cache_hit)
        )
    }
}

/// Aggregated flush worker metrics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlushMetrics {
    /// Number of write batches processed by the flush worker.
    pub batch_count: u64,
    /// Number of batches that required a filesystem sync.
    pub sync_batch_count: u64,
    /// Number of write requests included in all batches.
    pub write_request_count: u64,
    /// Total bytes written by the flush worker.
    pub write_bytes: u64,
    /// Number of callbacks sent after write batches.
    pub callback_count: u64,
    /// Number of times the flush worker intentionally waited for batching.
    pub group_wait_count: u64,
    /// Total intentional group-commit wait time, in microseconds.
    pub group_wait_us: u64,
    /// Maximum intentional group-commit wait time, in microseconds.
    pub group_wait_max_us: u64,
    /// Total request queue wait before a batch starts writing, in
    /// microseconds.
    pub queued_wait_us: u64,
    /// Maximum request queue wait before a batch starts writing, in
    /// microseconds.
    pub queued_wait_max_us: u64,
    /// Total file write time, in microseconds.
    pub write_us: u64,
    /// Maximum file write time for one batch, in microseconds.
    pub write_max_us: u64,
    /// Total filesystem sync time, in microseconds.
    pub sync_us: u64,
    /// Maximum filesystem sync time for one batch, in microseconds.
    pub sync_max_us: u64,
    /// Total batch processing time, in microseconds.
    pub batch_us: u64,
    /// Maximum batch processing time, in microseconds.
    pub batch_max_us: u64,
    /// Largest number of write requests in one batch.
    pub batch_size_max: u64,
    /// Largest number of bytes written by one batch.
    pub batch_bytes_max: u64,
    /// Number of write requests in the latest batch.
    pub last_batch_size: u64,
    /// Number of bytes written by the latest batch.
    pub last_batch_bytes: u64,
    /// Number of callbacks sent by the latest batch.
    pub last_callback_count: u64,
    /// Filesystem sync duration of the latest batch, in microseconds.
    pub last_sync_us: u64,
    /// Request queue wait maximum of the latest batch, in microseconds.
    pub last_queued_wait_max_us: u64,
    /// Intentional group-commit wait latency percentiles, in microseconds.
    pub group_wait_percentiles: FlushLatencyPercentiles,
    /// Per-batch max request queue wait latency percentiles, in microseconds.
    pub queued_wait_percentiles: FlushLatencyPercentiles,
    /// File write latency percentiles, in microseconds.
    pub write_percentiles: FlushLatencyPercentiles,
    /// Filesystem sync latency percentiles, in microseconds.
    pub sync_percentiles: FlushLatencyPercentiles,
    /// Whole batch processing latency percentiles, in microseconds.
    pub batch_percentiles: FlushLatencyPercentiles,
}

/// Percentiles for one flush worker latency dimension.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlushLatencyPercentiles {
    pub p50_us: u64,
    pub p90_us: u64,
    pub p99_us: u64,
}

/// Statistics about a single chunk in the Raft log
#[derive(Debug, Clone)]
pub struct ChunkStat<C> {
    /// Unique identifier for this chunk
    pub chunk_id: ChunkId,
    /// Number of records stored in this chunk
    pub records_count: u64,
    /// Global index of the first record in this chunk
    pub global_start: u64,
    /// Global index of the last record in this chunk plus one
    pub global_end: u64,
    /// Size of the chunk in bytes
    pub size: u64,
    /// Checkpoint stored for this chunk.
    pub log_state: C,
}

impl<C> fmt::Display for ChunkStat<C>
where C: fmt::Debug
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ChunkStat({}){{records: {}, [{}, {}), size: {}, log_state: {:?}}}",
            self.chunk_id,
            self.records_count,
            format_pad9_u64(self.global_start),
            format_pad9_u64(self.global_end),
            format_pad9_u64(self.size),
            self.log_state
        )
    }
}
