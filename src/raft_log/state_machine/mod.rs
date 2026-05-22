use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::RwLock;

use payload_cache::PayloadCache;
use raft_log_state::RaftLogState;

use crate::ChunkId;
use crate::Config;
use crate::RaftLogRecord;
use crate::Types;
use crate::WALRecord;
use crate::api::state_machine::StateMachine;
use crate::errors::RaftLogStateError;
use crate::raft_log::log_data::LogData;
use crate::raft_log::raft_log_action::RaftLogAction;
use crate::types::Segment;

pub(crate) mod payload_cache;
pub mod raft_log_state;

#[derive(Debug)]
pub struct RaftLogStateMachine<T: Types> {
    pub(crate) log: BTreeMap<u64, LogData<T>>,
    pub(crate) payload_cache: Arc<RwLock<PayloadCache<T>>>,
    pub(crate) log_state: RaftLogState<T>,
}

impl<T: Types> RaftLogStateMachine<T> {
    pub fn new(config: &Config) -> Self {
        Self {
            log: BTreeMap::new(),
            payload_cache: Arc::new(RwLock::new(PayloadCache::new(
                config.log_cache_max_items(),
                config.log_cache_capacity(),
            ))),
            log_state: RaftLogState::default(),
        }
    }
}

impl<T: Types> StateMachine<RaftLogRecord<T>> for RaftLogStateMachine<T> {
    type Error = RaftLogStateError<T>;
    type Checkpoint = RaftLogState<T>;

    fn apply(
        &mut self,
        rec: &RaftLogRecord<T>,
        chunk_id: ChunkId,
        segment: Segment,
    ) -> Result<(), RaftLogStateError<T>> {
        match rec {
            WALRecord::Action(RaftLogAction::SaveVote(_vote)) => {}
            WALRecord::Action(RaftLogAction::Append(log_id, payload)) => {
                self.log.insert(
                    T::log_index(log_id),
                    LogData::new(log_id.clone(), chunk_id, segment),
                );
                self.payload_cache
                    .write()
                    .unwrap()
                    .insert(log_id.clone(), payload.clone());
            }
            WALRecord::Action(RaftLogAction::Commit(_committed)) => {}
            WALRecord::Action(RaftLogAction::TruncateAfter(log_id)) => {
                let index = T::next_log_index(log_id.as_ref());
                self.log.split_off(&index);
                if let Some(log_id) = log_id {
                    self.payload_cache.write().unwrap().truncate_after(log_id);
                } else {
                    self.payload_cache.write().unwrap().clear();
                }
            }
            WALRecord::Action(RaftLogAction::PurgeUpto(log_id)) => {
                let index = T::next_log_index(Some(log_id));
                let b = self.log.split_off(&index);
                self.log = b;

                self.payload_cache.write().unwrap().purge_upto(log_id);
            }
            WALRecord::Checkpoint(_st) => {}
        }

        self.log_state.apply(rec)
    }

    fn checkpoint(&self) -> Self::Checkpoint {
        self.log_state.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::ChunkId;
    use crate::Config;
    use crate::RaftLogRecord;
    use crate::api::state_machine::StateMachine;
    use crate::errors::RaftLogStateError;
    use crate::raft_log::raft_log_action::RaftLogAction;
    use crate::raft_log::state_machine::RaftLogStateMachine;
    use crate::raft_log::state_machine::raft_log_state::RaftLogState;
    use crate::testing::TestTypes;
    use crate::testing::ss;
    use crate::types::Segment;

    #[test]
    fn test_checkpoint_returns_current_log_state()
    -> Result<(), RaftLogStateError<TestTypes>> {
        let mut sm = RaftLogStateMachine::<TestTypes>::new(&Config::default());
        let segment = Segment::new(0, 0);

        sm.apply(
            &RaftLogRecord::Action(RaftLogAction::SaveVote((2, 1))),
            ChunkId(0),
            segment,
        )?;
        sm.apply(
            &RaftLogRecord::Action(RaftLogAction::Append(
                (3, 7),
                ss("payload"),
            )),
            ChunkId(0),
            segment,
        )?;
        sm.apply(
            &RaftLogRecord::Action(RaftLogAction::Commit((3, 7))),
            ChunkId(0),
            segment,
        )?;

        assert_eq!(
            RaftLogState {
                vote: Some((2, 1)),
                last: Some((3, 7)),
                committed: Some((3, 7)),
                purged: None,
                user_data: None,
            },
            sm.checkpoint()
        );

        Ok(())
    }
}
