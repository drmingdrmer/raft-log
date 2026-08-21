use std::collections::BTreeMap;
use std::io;

use chunked_wal::ClosedChunkReader;

use crate::RaftWalTypes;
use crate::Types;
use crate::WALRecord;
use crate::raft_log::log_data::LogData;
use crate::raft_log::raft_log_action::RaftLogAction;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;

/// A struct that contains a snapshot of RaftLog data for inspection or
/// debugging.
///
/// It includes the log state, log entries, cache entries and closed chunks.
pub struct DumpRaftLog<T: Types> {
    pub(crate) state: RaftLogState<T>,

    pub(crate) logs: Vec<LogData<T>>,
    pub(crate) cache: BTreeMap<T::LogId, T::LogPayload>,
    pub(crate) record_reader: ClosedChunkReader<RaftWalTypes<T>>,

    pub(crate) cache_hit: usize,
    pub(crate) cache_miss: usize,
}

impl<T: Types> DumpRaftLog<T> {
    /// Returns a reference to the RaftLog state machine state
    pub fn state(&self) -> &RaftLogState<T> {
        &self.state
    }

    /// Returns an iterator that yields log entries in order
    ///
    /// The iterator yields Result<(log_id, payload), io::Error> pairs. The
    /// payload is retrieved either from cache or by reading from the
    /// underlying chunk storage.
    pub fn iter(&mut self) -> DumpRaftLogIter<'_, T> {
        DumpRaftLogIter { i: 0, data: self }
    }
}

/// An iterator over log entries in a DumpData
///
/// It yields Result<(log_id, payload), io::Error> pairs. The payload is
/// retrieved either from cache or by reading from the underlying chunk storage.
///
/// # Errors
/// The iterator may return io::Error if:
/// - The chunk containing a log entry is not found
/// - There is an error reading a record from storage
pub struct DumpRaftLogIter<'a, T: Types> {
    i: usize,
    data: &'a mut DumpRaftLog<T>,
}

impl<T: Types> DumpRaftLogIter<'_, T> {
    /// Reads a log payload from the chunk storage
    ///
    /// # Errors
    /// Returns io::Error if:
    /// - The chunk is not found
    /// - There is an error reading the record
    fn read_log_payload(
        &self,
        data: &LogData<T>,
    ) -> Result<T::LogPayload, io::Error> {
        let chunk_id = data.chunk_id;
        let segment = data.record_segment;
        let record = self.data.record_reader.read_record(chunk_id, segment)?;

        if let WALRecord::Action(RaftLogAction::Append(log_id, payload)) =
            record
        {
            debug_assert_eq!(log_id, data.log_id);
            Ok(payload)
        } else {
            panic!("Expect Record::Append but: {:?}", record);
        }
    }
}

impl<T: Types> Iterator for DumpRaftLogIter<'_, T> {
    type Item = Result<(T::LogId, T::LogPayload), io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.i >= self.data.logs.len() {
            return None;
        }

        let data = &self.data.logs[self.i];
        self.i += 1;

        let log_id = data.log_id.clone();
        let payload = self.data.cache.get(&log_id).cloned();

        if let Some(payload) = payload {
            self.data.cache_hit += 1;
            Some(Ok((log_id, payload)))
        } else {
            self.data.cache_miss += 1;
            Some(self.read_log_payload(data).map(|payload| (log_id, payload)))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use crate::raft_log::state_machine::raft_log_state::RaftLogState;
    use crate::testing::ss;
    use crate::tests::context::TestContext;
    use crate::tests::sample_data::build_sample_data;

    #[test]
    fn test_dump_data() -> Result<(), io::Error> {
        let mut ctx = TestContext::new()?;
        let config = &mut ctx.config;

        config.wal.chunk_max_records = Some(5);
        config.log_cache_max_items = Some(3);

        let mut rl = ctx.new_raft_log()?;

        build_sample_data(&mut rl)?;

        let mut data = rl.dump_data();
        assert_eq!(data.state(), &RaftLogState {
            vote: None,
            last: Some((2, 7)),
            committed: Some((1, 2)),
            purged: Some((1, 1)),
            user_data: None,
        });

        let mut iter = data.iter();

        let mut actual = vec![];
        while let Some(Ok((log_id, payload))) = iter.next() {
            actual.push(format!("{:?}: {}", log_id, payload));
        }

        let want = vec![
            ss("(2, 2): world"),
            ss("(2, 3): foo"),
            ss("(2, 4): world"),
            ss("(2, 5): foo"),
            ss("(2, 6): bar"),
            ss("(2, 7): wow"),
        ];

        assert_eq!(actual, want);

        assert_eq!(data.cache_hit, 4);
        assert_eq!(data.cache_miss, 2);

        Ok(())
    }
}
