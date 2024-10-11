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
}

impl<T: Types> codeq::Encode for RaftLogState<T> {
    fn encode<W: io::Write>(&self, mut w: W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.vote.encode(&mut w)?;
        n += self.last.encode(&mut w)?;
        n += self.committed.encode(&mut w)?;
        n += self.purged.encode(&mut w)?;

        Ok(n)
    }
}

impl<T: Types> codeq::Decode for RaftLogState<T> {
    fn decode<R: io::Read>(mut r: R) -> Result<Self, io::Error> {
        let vote = codeq::Decode::decode(&mut r)?;
        let last = codeq::Decode::decode(&mut r)?;
        let committed = codeq::Decode::decode(&mut r)?;
        let purged = codeq::Decode::decode(&mut r)?;

        Ok(Self {
            vote,
            last,
            committed,
            purged,
        })
    }
}

impl<T: Types> Default for RaftLogState<T> {
    fn default() -> Self {
        Self {
            vote: None,
            last: None,
            committed: None,
            purged: None,
        }
    }
}

impl<T: Types> RaftLogState<T> {
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

        let expected = T::next_log_index(self.last.as_ref());
        let this_index = T::get_log_index(log_id);

        if expected != this_index {
            return Err(LogIdNonConsecutive::new(
                self.last.clone(),
                log_id.clone(),
            )
            .into());
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
        if self.purged.as_ref() < Some(log_id) {
            self.purged = Some(log_id.clone());
        }
        Ok(())
    }
}
