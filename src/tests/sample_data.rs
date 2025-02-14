//! Provide sample data for testing
//!
//! This module contains functions for building sample data that can be used
//! to test the RaftLog implementation.
//!
//! The sample data is used to test the RaftLog implementation under various
//! conditions.

use std::io;
use std::thread::sleep;
use std::time::Duration;

use indoc::indoc;
use pretty_assertions::assert_eq;

use crate::api::raft_log_writer::blocking_flush;
use crate::api::raft_log_writer::RaftLogWriter;
use crate::testing::ss;
use crate::testing::TestTypes;
use crate::DumpApi;
use crate::RaftLog;

pub fn build_sample_data_purge_upto_3(
    rl: &mut RaftLog<TestTypes>,
) -> Result<String, io::Error> {
    build_sample_data(rl)?;

    rl.purge((2, 3))?;
    blocking_flush(rl)?;

    let dumped = indoc! {r#"
        RaftLog:
        ChunkId(00_000_000_000_000_000_324)
          R-00000: [000_000_000, 000_000_050) Size(50): State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None, user_data: None })
          R-00001: [000_000_050, 000_000_078) Size(28): PurgeUpto((1, 1))
          R-00002: [000_000_078, 000_000_115) Size(37): Append((2, 4), "world")
          R-00003: [000_000_115, 000_000_150) Size(35): Append((2, 5), "foo")
          R-00004: [000_000_150, 000_000_185) Size(35): Append((2, 6), "bar")
        ChunkId(00_000_000_000_000_000_509)
          R-00000: [000_000_000, 000_000_066) Size(66): State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None })
          R-00001: [000_000_066, 000_000_101) Size(35): Append((2, 7), "wow")
          R-00002: [000_000_101, 000_000_129) Size(28): PurgeUpto((2, 3))
        "#};

    let dump = rl.dump().write_to_string()?;
    println!("After purge:\n{}", dump);

    assert_eq!(dumped, dump);

    // Wait for FlushWorker to quit and remove purged chunks
    sleep(Duration::from_millis(100));

    Ok(dumped.to_string())
}

pub fn build_sample_data(
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
          R-00000: [000_000_000, 000_000_018) Size(18): State(RaftLogState { vote: None, last: None, committed: None, purged: None, user_data: None })
          R-00001: [000_000_018, 000_000_052) Size(34): Append((1, 0), "hi")
          R-00002: [000_000_052, 000_000_089) Size(37): Append((1, 1), "hello")
          R-00003: [000_000_089, 000_000_126) Size(37): Append((1, 2), "world")
          R-00004: [000_000_126, 000_000_161) Size(35): Append((1, 3), "foo")
        ChunkId(00_000_000_000_000_000_161)
          R-00000: [000_000_000, 000_000_034) Size(34): State(RaftLogState { vote: None, last: Some((1, 3)), committed: None, purged: None, user_data: None })
          R-00001: [000_000_034, 000_000_063) Size(29): TruncateAfter(Some((1, 1)))
          R-00002: [000_000_063, 000_000_100) Size(37): Append((2, 2), "world")
          R-00003: [000_000_100, 000_000_135) Size(35): Append((2, 3), "foo")
          R-00004: [000_000_135, 000_000_163) Size(28): Commit((1, 2))
        ChunkId(00_000_000_000_000_000_324)
          R-00000: [000_000_000, 000_000_050) Size(50): State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None, user_data: None })
          R-00001: [000_000_050, 000_000_078) Size(28): PurgeUpto((1, 1))
          R-00002: [000_000_078, 000_000_115) Size(37): Append((2, 4), "world")
          R-00003: [000_000_115, 000_000_150) Size(35): Append((2, 5), "foo")
          R-00004: [000_000_150, 000_000_185) Size(35): Append((2, 6), "bar")
        ChunkId(00_000_000_000_000_000_509)
          R-00000: [000_000_000, 000_000_066) Size(66): State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None })
          R-00001: [000_000_066, 000_000_101) Size(35): Append((2, 7), "wow")
        "#};
    Ok(dumped.to_string())
}
