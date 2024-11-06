use std::io;

use crate::api::types::Types;
use crate::errors::LogIdNonConsecutive;
use crate::errors::LogIdReversal;
use crate::errors::RaftLogStateError;
use crate::errors::VoteReversal;
use crate::raft_log::wal::wal_record::WALRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaftLogState<T: Types> {
    pub(crate) vote: Option<T::Vote>,

    pub(crate) last: Option<T::LogId>,
    pub(crate) committed: Option<T::LogId>,
    pub(crate) purged: Option<T::LogId>,

    pub user_data: Option<T::UserData>,
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

    pub(crate) fn apply(
        &mut self,
        rec: &WALRecord<T>,
    ) -> Result<(), RaftLogStateError<T>> {
        match rec {
            WALRecord::SaveVote(vote) => {
                self.update_vote(vote)?;
            }
            WALRecord::Append(log_id, _payload) => {
                self.append(log_id)?;
            }
            WALRecord::Commit(log_id) => {
                self.commit(log_id)?;
            }
            WALRecord::TruncateAfter(log_id) => {
                self.truncate_after(log_id.as_ref())?;
            }
            WALRecord::PurgeUpto(log_id) => {
                self.purge(log_id)?;
            }
            WALRecord::State(state) => {
                *self = state.clone();
            }
        }
        Ok(())
    }

    pub(crate) fn update_vote(
        &mut self,
        vote: &T::Vote,
    ) -> Result<(), RaftLogStateError<T>> {
        if Some(vote) >= self.vote.as_ref() {
            self.vote = Some(vote.clone());
        } else {
            return Err(VoteReversal::new(
                self.vote.clone().unwrap(),
                vote.clone(),
            )
            .into());
        }
        Ok(())
    }

    pub(crate) fn append(
        &mut self,
        log_id: &T::LogId,
    ) -> Result<(), RaftLogStateError<T>> {
        if Some(log_id) <= self.last.as_ref() {
            return Err(LogIdReversal::new(
                self.last.clone().unwrap(),
                log_id.clone(),
                "append",
            )
            .into());
        }

        // Do not check for consecutive log_id if last is None;
        // Because it's common to append the first log with non-zero index,
        // such as, when restoring a RaftLog.
        if self.last.is_some() {
            let expected = T::next_log_index(self.last.as_ref());
            let this_index = T::log_index(log_id);

            if expected != this_index {
                return Err(LogIdNonConsecutive::new(
                    self.last.clone(),
                    log_id.clone(),
                )
                .into());
            }
        }

        self.last = Some(log_id.clone());
        Ok(())
    }

    pub(crate) fn commit(
        &mut self,
        log_id: &T::LogId,
    ) -> Result<(), RaftLogStateError<T>> {
        if Some(log_id) < self.committed.as_ref() {
            return Err(LogIdReversal::new(
                self.committed.clone().unwrap(),
                log_id.clone(),
                "commit",
            )
            .into());
        }

        self.committed = Some(log_id.clone());
        Ok(())
    }

    pub(crate) fn truncate_after(
        &mut self,
        log_id: Option<&T::LogId>,
    ) -> Result<(), RaftLogStateError<T>> {
        if self.last.as_ref() > log_id {
            self.last = log_id.cloned();
        }
        Ok(())
    }

    pub(crate) fn purge(
        &mut self,
        log_id: &T::LogId,
    ) -> Result<(), RaftLogStateError<T>> {
        let purged = Some(log_id.clone());

        if self.purged < purged {
            self.purged.clone_from(&purged);
        }

        if purged > self.last {
            self.last = purged;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use crate::raft_log::state_machine::raft_log_state::RaftLogState;
    use crate::testing::ss;
    use crate::testing::test_codec_without_corruption;
    use crate::testing::TestTypes;

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
}
