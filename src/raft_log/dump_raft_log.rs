use std::collections::BTreeMap;
use std::io;

use crate::chunk::closed_chunk::ClosedChunk;
use crate::raft_log::log_data::LogData;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;
use crate::ChunkId;
use crate::Types;
use crate::WALRecord;

/// A struct that contains a snapshot of RaftLog data for inspection or
/// debugging.
///
/// It includes the log state, log entries, cache entries and closed chunks.
pub struct DumpRaftLog<T: Types> {
    pub(crate) state: RaftLogState<T>,

    pub(crate) logs: Vec<LogData<T>>,
    pub(crate) cache: BTreeMap<T::LogId, T::LogPayload>,
    pub(crate) chunks: BTreeMap<ChunkId, ClosedChunk<T>>,

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
        let closed = self.data.chunks.get(&chunk_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "Chunk not found: {}; when:(DumpRaftLogIter open cache-miss read)",
                    chunk_id
                ),
            )
        })?;

        let record = closed.chunk.read_record(segment)?;

        if let WALRecord::Append(log_id, payload) = record {
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

    use indoc::indoc;

    use crate::api::raft_log_writer::blocking_flush;
    use crate::api::raft_log_writer::RaftLogWriter;
    use crate::raft_log::state_machine::raft_log_state::RaftLogState;
    use crate::testing::ss;
    use crate::testing::TestTypes;
    use crate::tests::context::TestContext;
    use crate::RaftLog;

    #[test]
    fn test_dump_data() -> Result<(), io::Error> {
        let mut ctx = TestContext::new()?;
        let config = &mut ctx.config;

        config.chunk_max_records = Some(5);
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

    fn build_sample_data(
        rl: &mut RaftLog<TestTypes>,
    ) -> Result<String, io::Error> {
        assert_eq!(rl.config.chunk_max_records, Some(5));

        let logs = [
            //
            ((1, 0), ss("hi")),
            ((1, 1), ss("hello")),
            ((1, 2), ss("world")),
            ((1, 3), ss("foo")),
        ];
        rl.append(logs)?;

        rl.truncate(2)?;

        let logs = [
            //
            ((2, 2), ss("world")),
            ((2, 3), ss("foo")),
        ];
        rl.append(logs)?;

        rl.commit((1, 2))?;
        rl.purge((1, 1))?;
        blocking_flush(rl)?;

        let logs = [
            //
            ((2, 4), ss("world")),
            ((2, 5), ss("foo")),
            ((2, 6), ss("bar")),
            ((2, 7), ss("wow")),
        ];
        rl.append(logs)?;

        blocking_flush(rl)?;

        let dumped = indoc! {r#"
        RaftLog:
        ChunkId(00_000_000_000_000_000_000)
          R-00000: [000_000_000, 000_000_018) 18: State(RaftLogState { vote: None, last: None, committed: None, purged: None, user_data: None })
          R-00001: [000_000_018, 000_000_052) 34: Append((1, 0), "hi")
          R-00002: [000_000_052, 000_000_089) 37: Append((1, 1), "hello")
          R-00003: [000_000_089, 000_000_126) 37: Append((1, 2), "world")
          R-00004: [000_000_126, 000_000_161) 35: Append((1, 3), "foo")
        ChunkId(00_000_000_000_000_000_161)
          R-00000: [000_000_000, 000_000_034) 34: State(RaftLogState { vote: None, last: Some((1, 3)), committed: None, purged: None, user_data: None })
          R-00001: [000_000_034, 000_000_063) 29: TruncateAfter(Some((1, 1)))
          R-00002: [000_000_063, 000_000_100) 37: Append((2, 2), "world")
          R-00003: [000_000_100, 000_000_135) 35: Append((2, 3), "foo")
          R-00004: [000_000_135, 000_000_163) 28: Commit((1, 2))
        ChunkId(00_000_000_000_000_000_324)
          R-00000: [000_000_000, 000_000_050) 50: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None, user_data: None })
          R-00001: [000_000_050, 000_000_078) 28: PurgeUpto((1, 1))
          R-00002: [000_000_078, 000_000_115) 37: Append((2, 4), "world")
          R-00003: [000_000_115, 000_000_150) 35: Append((2, 5), "foo")
          R-00004: [000_000_150, 000_000_185) 35: Append((2, 6), "bar")
        ChunkId(00_000_000_000_000_000_509)
          R-00000: [000_000_000, 000_000_066) 66: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None })
          R-00001: [000_000_066, 000_000_101) 35: Append((2, 7), "wow")
        "#};
        Ok(dumped.to_string())
    }
}
