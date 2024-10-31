use std::collections::BTreeMap;

use codeq::Segment;
use payload_cache::PayloadCache;
use raft_log_state::RaftLogState;

use crate::api::state_machine::StateMachine;
use crate::errors::RaftLogStateError;
use crate::raft_log::log_data::LogData;
use crate::ChunkId;
use crate::Config;
use crate::Types;
use crate::WALRecord;

pub(crate) mod payload_cache;
pub mod raft_log_state;

#[derive(Debug)]
pub struct RaftLogStateMachine<T: Types> {
    pub(crate) log: BTreeMap<u64, LogData<T>>,
    pub(crate) payload_cache: PayloadCache<T>,
    pub(crate) log_state: RaftLogState<T>,
}

impl<T: Types> RaftLogStateMachine<T> {
    pub fn new(config: &Config) -> Self {
        Self {
            log: BTreeMap::new(),
            payload_cache: PayloadCache::new(
                config.log_cache_max_items(),
                config.log_cache_capacity(),
            ),
            log_state: RaftLogState::default(),
        }
    }
}

impl<T: Types> StateMachine<WALRecord<T>> for RaftLogStateMachine<T> {
    type Error = RaftLogStateError<T>;

    fn apply(
        &mut self,
        rec: &WALRecord<T>,
        chunk_id: ChunkId,
        segment: Segment,
    ) -> Result<(), RaftLogStateError<T>> {
        match rec {
            WALRecord::SaveVote(_vote) => {}
            WALRecord::Append(log_id, payload) => {
                self.log.insert(
                    T::log_index(log_id),
                    LogData::new(log_id.clone(), chunk_id, segment),
                );
                self.payload_cache.insert(log_id.clone(), payload.clone());
            }
            WALRecord::Commit(_committed) => {}
            WALRecord::TruncateAfter(log_id) => {
                let index = T::next_log_index(log_id.as_ref());
                self.log.split_off(&index);
                if let Some(log_id) = log_id {
                    self.payload_cache.truncate_after(log_id);
                } else {
                    self.payload_cache.clear();
                }
            }
            WALRecord::PurgeUpto(log_id) => {
                let index = T::next_log_index(Some(log_id));
                let b = self.log.split_off(&index);
                self.log = b;
            }
            WALRecord::State(_st) => {}
        }

        self.log_state.apply(rec)
    }
}
