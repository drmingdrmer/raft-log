use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::RwLock;

use payload_cache::PayloadCache;
use raft_log_state::RaftLogState;

use crate::ChunkId;
use crate::Config;
use crate::RaftLogRecord;
use crate::RaftWalTypes;
use crate::Types;
use crate::WALRecord;
use crate::WalTypes;
use crate::api::state_machine::StateMachine;
use crate::errors::CheckpointLogMismatch;
use crate::errors::LogIdIndexDisorder;
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

    /// Verify that `upto` names a log id this log actually holds.
    ///
    /// A Raft log stores a greater log id at a greater index, so the log id
    /// sitting at `log_index(upto)` is already determined by what was
    /// written. When `upto` disagrees with it, the caller is purging by a log
    /// id this log never had, and purging anyway would either drop entries the
    /// state still counts as present or keep entries the state counts as
    /// purged. Both leave the log permanently inconsistent, so the call is
    /// rejected instead.
    pub(crate) fn check_purge(
        &self,
        upto: &T::LogId,
    ) -> Result<(), RaftLogStateError<T>> {
        let stored = self.log.get(&T::log_index(upto)).map(|d| &d.log_id);

        Self::check_index_order(upto, stored)?;
        Self::check_index_order(upto, self.log_state.purged.as_ref())?;
        Self::check_index_order(upto, self.log_state.last.as_ref())?;
        Ok(())
    }

    /// Verify that `state` describes the log entries this store actually
    /// holds.
    ///
    /// A checkpoint declares the log to be exactly the entries in
    /// `(purged, last]`. The log map is contiguous, so that reduces to two
    /// conditions: its first entry sits right after `purged`, and its last
    /// entry equals `last`.
    ///
    /// An empty log map matches any state. That is what a store restored from
    /// a snapshot needs, because it declares its state before it holds a
    /// single entry.
    ///
    /// Only a live store can be checked this way, so [`RaftLog::update_state`]
    /// is the sole caller and [`StateMachine::apply`] does not run this. A
    /// chunk-leading checkpoint carries the `purged` of the moment its chunk
    /// was created; a later purge then deletes chunks, so on replay the map
    /// legitimately lacks entries that the older checkpoint still counts as
    /// live. Replay cannot tell that apart from a checkpoint that was wrong
    /// when written.
    pub(crate) fn check_checkpoint(
        &self,
        state: &RaftLogState<T>,
    ) -> Result<(), RaftLogStateError<T>> {
        let Some((_, first)) = self.log.first_key_value() else {
            return Ok(());
        };
        let (_, last) = self.log.last_key_value().unwrap();

        let purged = state.purged.as_ref();
        let first_index = T::log_index(&first.log_id);
        let index_after_purged = T::next_log_index(purged);

        let starts_at_right_index = first_index == index_after_purged;
        let starts_above_purged = Some(&first.log_id) > purged;
        let ends_at_last = Some(&last.log_id) == state.last.as_ref();

        if starts_at_right_index && starts_above_purged && ends_at_last {
            return Ok(());
        }

        let err = CheckpointLogMismatch::new(
            first.log_id.clone(),
            last.log_id.clone(),
            state.purged.clone(),
            state.last.clone(),
        );
        Err(err.into())
    }

    /// Require `attempted` and `stored` to order the same way by log id and by
    /// index.
    ///
    /// When the two indexes are equal this demands the two log ids be equal,
    /// which is the check against the entry stored at that index.
    fn check_index_order(
        attempted: &T::LogId,
        stored: Option<&T::LogId>,
    ) -> Result<(), RaftLogStateError<T>> {
        let Some(stored) = stored else {
            return Ok(());
        };

        let by_log_id = attempted.cmp(stored);
        let by_index = T::log_index(attempted).cmp(&T::log_index(stored));

        if by_log_id != by_index {
            let err = LogIdIndexDisorder::new(
                stored.clone(),
                attempted.clone(),
                "purge",
            );
            return Err(err.into());
        }

        Ok(())
    }
}

impl<T: Types> StateMachine<RaftWalTypes<T>> for RaftLogStateMachine<T> {
    type Error = RaftLogStateError<T>;

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
                self.check_purge(log_id)?;

                let index = T::next_log_index(Some(log_id));
                let b = self.log.split_off(&index);
                self.log = b;

                self.payload_cache.write().unwrap().purge_upto(log_id);
            }
            WALRecord::Checkpoint(_st) => {}
        }

        self.log_state.apply(rec)
    }

    fn checkpoint(&self) -> <RaftWalTypes<T> as WalTypes>::Checkpoint {
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

    /// Replaying a WAL that holds a purge record conflicting with the log id
    /// stored at that index must fail, so a corrupt log is reported at
    /// `RaftLog::open` instead of rebuilt into an inconsistent state.
    #[test]
    fn test_apply_rejects_purge_conflicting_with_stored_log()
    -> Result<(), RaftLogStateError<TestTypes>> {
        let mut sm = RaftLogStateMachine::<TestTypes>::new(&Config::default());
        let segment = Segment::new(0, 0);

        sm.apply(
            &RaftLogRecord::Action(RaftLogAction::Append((1, 7), ss("a"))),
            ChunkId(0),
            segment,
        )?;

        let record = RaftLogRecord::Action(RaftLogAction::PurgeUpto((2, 7)));
        let err = sm.apply(&record, ChunkId(0), segment).unwrap_err();
        let want = "Log id conflicts with the stored log id when purge: stored (1, 7), attempted (2, 7); log id order and log index order must agree";
        assert_eq!(want, err.to_string());

        Ok(())
    }
}
