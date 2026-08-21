use std::fmt;
use std::fmt::Formatter;
use std::io;

use display_more::DisplayOptionExt;

use crate::RaftLogRecord;
use crate::WALRecord;
use crate::api::types::Types;
use crate::errors::LogIdNonConsecutive;
use crate::errors::LogIdReversal;
use crate::errors::RaftLogStateError;
use crate::errors::VoteReversal;
use crate::raft_log::raft_log_action::RaftLogAction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaftLogState<T: Types> {
    pub(crate) vote: Option<T::Vote>,

    pub(crate) last: Option<T::LogId>,
    pub(crate) committed: Option<T::LogId>,
    pub(crate) purged: Option<T::LogId>,

    pub user_data: Option<T::UserData>,
}

impl<T> fmt::Display for RaftLogState<T>
where
    T: Types,
    T::Vote: fmt::Display,
    T::LogId: fmt::Display,
    T::LogPayload: fmt::Display,
    T::UserData: fmt::Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RaftLogState(vote: {}, last: {}, committed: {}, purged: {}, user_data: {})",
            self.vote.display(),
            self.last.display(),
            self.committed.display(),
            self.purged.display(),
            self.user_data.display()
        )
    }
}

impl<T: Types> codeq::Encode for RaftLogState<T> {
    fn encode<W: io::Write>(&self, mut w: W) -> Result<usize, io::Error> {
        let mut n = 0;

        let ver = 1u8;
        n += ver.encode(&mut w)?;

        n += self.vote.encode(&mut w)?;
        n += self.last.encode(&mut w)?;
        n += self.committed.encode(&mut w)?;
        n += self.purged.encode(&mut w)?;
        n += self.user_data.encode(&mut w)?;

        Ok(n)
    }
}

impl<T: Types> codeq::Decode for RaftLogState<T> {
    fn decode<R: io::Read>(mut r: R) -> Result<Self, io::Error> {
        let ver: u8 = codeq::Decode::decode(&mut r)?;

        match ver {
            1 => {
                let vote = codeq::Decode::decode(&mut r)?;
                let last = codeq::Decode::decode(&mut r)?;
                let committed = codeq::Decode::decode(&mut r)?;
                let purged = codeq::Decode::decode(&mut r)?;
                let user_data = codeq::Decode::decode(&mut r)?;

                Ok(Self {
                    vote,
                    last,
                    committed,
                    purged,
                    user_data,
                })
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsupported RaftLogState version: {}", ver),
            )),
        }
    }
}

impl<T: Types> Default for RaftLogState<T> {
    fn default() -> Self {
        Self {
            vote: None,
            last: None,
            committed: None,
            purged: None,
            user_data: None,
        }
    }
}

impl<T: Types> RaftLogState<T> {
    pub fn vote(&self) -> Option<&T::Vote> {
        self.vote.as_ref()
    }

    pub fn last(&self) -> Option<&T::LogId> {
        self.last.as_ref()
    }

    pub fn committed(&self) -> Option<&T::LogId> {
        self.committed.as_ref()
    }

    pub fn purged(&self) -> Option<&T::LogId> {
        self.purged.as_ref()
    }

    pub fn set_last(&mut self, log_id: Option<T::LogId>) {
        self.last = log_id;
    }

    /// Verify that `rec` can be applied to this state, without changing it.
    ///
    /// [`Self::apply`] runs this first, so applying an accepted record never
    /// fails halfway.
    pub(crate) fn check(
        &self,
        rec: &RaftLogRecord<T>,
    ) -> Result<(), RaftLogStateError<T>> {
        match rec {
            WALRecord::Action(RaftLogAction::SaveVote(vote)) => {
                self.check_vote(vote)
            }
            WALRecord::Action(RaftLogAction::Append(log_id, _payload)) => {
                self.check_append(log_id)
            }
            WALRecord::Action(RaftLogAction::Commit(log_id)) => {
                self.check_commit(log_id)
            }
            WALRecord::Action(RaftLogAction::TruncateAfter(_log_id)) => Ok(()),
            WALRecord::Action(RaftLogAction::PurgeUpto(_log_id)) => Ok(()),
            WALRecord::Checkpoint(_state) => Ok(()),
        }
    }

    pub(crate) fn apply(
        &mut self,
        rec: &RaftLogRecord<T>,
    ) -> Result<(), RaftLogStateError<T>> {
        self.check(rec)?;

        match rec {
            WALRecord::Action(RaftLogAction::SaveVote(vote)) => {
                self.vote = Some(vote.clone());
            }
            WALRecord::Action(RaftLogAction::Append(log_id, _payload)) => {
                self.last = Some(log_id.clone());
            }
            WALRecord::Action(RaftLogAction::Commit(log_id)) => {
                self.committed = Some(log_id.clone());
            }
            WALRecord::Action(RaftLogAction::TruncateAfter(log_id)) => {
                self.truncate_after(log_id.as_ref());
            }
            WALRecord::Action(RaftLogAction::PurgeUpto(log_id)) => {
                self.purge(log_id);
            }
            WALRecord::Checkpoint(state) => {
                *self = state.clone();
            }
        }
        Ok(())
    }

    fn check_vote(&self, vote: &T::Vote) -> Result<(), RaftLogStateError<T>> {
        if Some(vote) >= self.vote.as_ref() {
            return Ok(());
        }

        let err = VoteReversal::new(self.vote.clone().unwrap(), vote.clone());
        Err(err.into())
    }

    fn check_append(
        &self,
        log_id: &T::LogId,
    ) -> Result<(), RaftLogStateError<T>> {
        if Some(log_id) <= self.last.as_ref() {
            let err = LogIdReversal::new(
                self.last.clone().unwrap(),
                log_id.clone(),
                "append",
            );
            return Err(err.into());
        }

        // Do not check for consecutive log_id if last is None;
        // Because it's common to append the first log with non-zero index,
        // such as, when restoring a RaftLog.
        if self.last.is_none() {
            return Ok(());
        }

        let expected = T::next_log_index(self.last.as_ref());
        let this_index = T::log_index(log_id);

        if expected != this_index {
            let err =
                LogIdNonConsecutive::new(self.last.clone(), log_id.clone());
            return Err(err.into());
        }

        Ok(())
    }

    fn check_commit(
        &self,
        log_id: &T::LogId,
    ) -> Result<(), RaftLogStateError<T>> {
        if Some(log_id) < self.committed.as_ref() {
            let err = LogIdReversal::new(
                self.committed.clone().unwrap(),
                log_id.clone(),
                "commit",
            );
            return Err(err.into());
        }

        Ok(())
    }

    fn truncate_after(&mut self, log_id: Option<&T::LogId>) {
        if self.last.as_ref() > log_id {
            self.last = log_id.cloned();
        }
    }

    fn purge(&mut self, log_id: &T::LogId) {
        let purged = Some(log_id.clone());

        if self.purged < purged {
            self.purged.clone_from(&purged);
        }

        if purged > self.last {
            self.last = purged;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use crate::raft_log::state_machine::raft_log_state::RaftLogState;
    use crate::testing::TestDisplayTypes;
    use crate::testing::TestTypes;
    use crate::testing::ss;
    use crate::testing::test_codec_without_corruption;

    #[test]
    fn test_raft_log_state_codec() -> Result<(), io::Error> {
        let state = RaftLogState::<TestTypes> {
            vote: Some((1, 2)),
            last: Some((2, 3)),
            committed: Some((4, 5)),
            purged: Some((6, 7)),
            user_data: Some(ss("hello")),
        };

        let b = vec![
            1, // version
            1, // Some
            0, 0, 0, 0, 0, 0, 0, 1, // vote.term
            0, 0, 0, 0, 0, 0, 0, 2, // vote.voted_for
            1, // Some
            0, 0, 0, 0, 0, 0, 0, 2, // last.term
            0, 0, 0, 0, 0, 0, 0, 3, // last.index
            1, // Some
            0, 0, 0, 0, 0, 0, 0, 4, // committed.term
            0, 0, 0, 0, 0, 0, 0, 5, // committed.index
            1, // Some
            0, 0, 0, 0, 0, 0, 0, 6, // purged.term
            0, 0, 0, 0, 0, 0, 0, 7, // purged.index
            1, // Some
            0, 0, 0, 5, // user_data.len
            104, 101, 108, 108, 111, // user_data
        ];

        test_codec_without_corruption(&b, &state)
    }

    #[test]
    fn test_raft_log_state_display() {
        //
        let state = RaftLogState::<TestDisplayTypes> {
            vote: Some(1),
            last: Some(2),
            committed: Some(4),
            purged: Some(6),
            user_data: Some("hello".to_string()),
        };

        let got = state.to_string();
        let want = "RaftLogState(vote: 1, last: 2, committed: 4, purged: 6, user_data: hello)";

        assert_eq!(want, got);
    }
}
