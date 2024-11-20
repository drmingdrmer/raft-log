use std::fmt;
use std::fmt::Formatter;

use crate::num::format_pad9_u64;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;
use crate::ChunkId;
use crate::Types;

#[derive(Debug, Clone)]
pub struct Stat<T>
where T: Types
{
    pub closed_chunks: Vec<ChunkStat<T>>,
    pub open_chunk: ChunkStat<T>,
    pub payload_cache_item_count: u64,
    pub payload_cache_max_item: u64,
    pub payload_cache_size: u64,
    pub payload_cache_capacity: u64,
    pub payload_cache_miss: u64,
    pub payload_cache_hit: u64,
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
            item_count: {},\
            max_item: {},\
            size: {},\
            capacity: {},\
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
            format_pad9_u64(self.payload_cache_item_count),
            format_pad9_u64(self.payload_cache_max_item),
            format_pad9_u64(self.payload_cache_size),
            format_pad9_u64(self.payload_cache_capacity),
            format_pad9_u64(self.payload_cache_miss),
            format_pad9_u64(self.payload_cache_hit)
        )
    }
}

#[derive(Debug, Clone)]
pub struct ChunkStat<T>
where T: Types
{
    pub chunk_id: ChunkId,
    pub records_count: u64,
    pub global_start: u64,
    pub global_end: u64,
    pub size: u64,
    pub log_state: RaftLogState<T>,
}

impl<T> fmt::Display for ChunkStat<T>
where T: Types
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
