use std::fmt;
use std::fmt::Formatter;

use display_more::DisplayDebugOptionExt;

use crate::ChunkStat;
use crate::FlushMetrics;
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
