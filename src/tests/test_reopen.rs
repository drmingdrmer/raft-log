//! Tests for reopening a RaftLog under various conditions.
//!
//! These tests verify that a RaftLog can be correctly reopened after:
//! - Normal shutdown
//! - Partial writes
//! - Corrupted chunks
//! - Missing chunks with zero-filled tail
//! - And other edge cases
//!
//! The tests ensure that the log state and entries are properly recovered.

use std::io;
use std::io::Seek;

use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use indoc::indoc;
use pretty_assertions::assert_eq;

use crate::chunk::Chunk;
use crate::testing::TestTypes;
use crate::tests::context::TestContext;
use crate::tests::sample_data;
use crate::ChunkId;
use crate::Dump;
use crate::DumpApi;

#[test]
fn test_re_open() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);

    let (state, logs) = {
        let mut rl = ctx.new_raft_log()?;
        sample_data::build_sample_data_purge_upto_3(&mut rl)?;

        (
            rl.log_state().clone(),
            rl.read(0, 1000).collect::<Result<Vec<_>, _>>()?,
        )
    };

    // Re-open
    {
        let rl = ctx.new_raft_log()?;

        assert_eq!(state, rl.log_state().clone());
        assert_eq!(
            logs,
            rl.read(0, 1000).collect::<Result<Vec<_>, io::Error>>()?
        );

        let dump = rl.dump().write_to_string()?;
        println!("After reopen:\n{}", dump);

        assert_eq!(
            indoc! {r#"
            RaftLog:
            ChunkId(00_000_000_000_000_000_324)
              R-00000: [000_000_000, 000_000_050) 50: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None, user_data: None })
              R-00001: [000_000_050, 000_000_078) 28: PurgeUpto((1, 1))
              R-00002: [000_000_078, 000_000_115) 37: Append((2, 4), "world")
              R-00003: [000_000_115, 000_000_150) 35: Append((2, 5), "foo")
              R-00004: [000_000_150, 000_000_185) 35: Append((2, 6), "bar")
            ChunkId(00_000_000_000_000_000_509)
              R-00000: [000_000_000, 000_000_066) 66: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None })
              R-00001: [000_000_066, 000_000_101) 35: Append((2, 7), "wow")
              R-00002: [000_000_101, 000_000_129) 28: PurgeUpto((2, 3))
            ChunkId(00_000_000_000_000_000_638)
              R-00000: [000_000_000, 000_000_066) 66: State(RaftLogState { vote: None, last: Some((2, 7)), committed: Some((1, 2)), purged: Some((2, 3)), user_data: None })
            "#},
            dump
        );
    }

    Ok(())
}

/// The last record will be discarded if it is not completely written.
#[test]
fn test_re_open_unfinished_chunk() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);

    let (mut state, logs) = {
        let mut rl = ctx.new_raft_log()?;
        sample_data::build_sample_data_purge_upto_3(&mut rl)?;

        (
            rl.log_state().clone(),
            rl.read(0, 1000).collect::<Result<Vec<_>, _>>()?,
        )
    };

    // Truncate the last record, the last record is at [99,127) size=28
    {
        let chunk_id = ChunkId(509);
        let f = Chunk::<TestTypes>::open_chunk_file(&ctx.config, chunk_id)?;
        f.set_len(126)?;

        // Last purge record will be discarded.
        state.purged = Some((1, 1));
    }

    // Re-open
    {
        let rl = ctx.new_raft_log()?;

        assert_eq!(state, rl.log_state().clone());
        assert_eq!(logs, rl.read(0, 1000).collect::<Result<Vec<_>, _>>()?);

        let dump = rl.dump().write_to_string()?;
        println!("After reopen:\n{}", dump);

        assert_eq!(
            indoc! {r#"
RaftLog:
ChunkId(00_000_000_000_000_000_324)
  R-00000: [000_000_000, 000_000_050) 50: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None, user_data: None })
  R-00001: [000_000_050, 000_000_078) 28: PurgeUpto((1, 1))
  R-00002: [000_000_078, 000_000_115) 37: Append((2, 4), "world")
  R-00003: [000_000_115, 000_000_150) 35: Append((2, 5), "foo")
  R-00004: [000_000_150, 000_000_185) 35: Append((2, 6), "bar")
ChunkId(00_000_000_000_000_000_509)
  R-00000: [000_000_000, 000_000_066) 66: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None })
  R-00001: [000_000_066, 000_000_101) 35: Append((2, 7), "wow")
ChunkId(00_000_000_000_000_000_610)
  R-00000: [000_000_000, 000_000_066) 66: State(RaftLogState { vote: None, last: Some((2, 7)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None })
"#},
            dump
        );
    }

    Ok(())
}

/// The last record will be discarded if it is not completely written and filled
/// with zeros.
///
/// Trailing zeros can happen if EXT4 is mounted with `data=writeback` mode,
/// with which, data and metadata(file len) will be written to disk in
/// arbitrary order.
#[test]
fn test_re_open_unfinished_tailing_zero_chunk() -> Result<(), io::Error> {
    for append_zeros in [3, 1024 * 33] {
        let mut ctx = TestContext::new()?;
        let config = &mut ctx.config;

        config.chunk_max_records = Some(5);

        let (state, logs) = {
            let mut rl = ctx.new_raft_log()?;
            sample_data::build_sample_data_purge_upto_3(&mut rl)?;

            (
                rl.log_state().clone(),
                rl.read(0, 1000).collect::<Result<Vec<_>, _>>()?,
            )
        };

        // Append several zero bytes
        {
            let chunk_id = ChunkId(509);
            let f = Chunk::<TestTypes>::open_chunk_file(&ctx.config, chunk_id)?;
            f.set_len(129 + append_zeros)?;
        }

        // Re-open
        {
            let rl = ctx.new_raft_log()?;

            let last_closed = rl.wal.closed.last_key_value().unwrap().1;
            assert_eq!(last_closed.chunk.truncated, Some(129 + append_zeros));

            assert_eq!(state, rl.log_state().clone());
            assert_eq!(logs, rl.read(0, 1000).collect::<Result<Vec<_>, _>>()?);

            let dump = rl.dump().write_to_string()?;
            println!("After reopen:\n{}", dump);

            assert_eq!(
                indoc! {r#"
RaftLog:
ChunkId(00_000_000_000_000_000_324)
  R-00000: [000_000_000, 000_000_050) 50: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None, user_data: None })
  R-00001: [000_000_050, 000_000_078) 28: PurgeUpto((1, 1))
  R-00002: [000_000_078, 000_000_115) 37: Append((2, 4), "world")
  R-00003: [000_000_115, 000_000_150) 35: Append((2, 5), "foo")
  R-00004: [000_000_150, 000_000_185) 35: Append((2, 6), "bar")
ChunkId(00_000_000_000_000_000_509)
  R-00000: [000_000_000, 000_000_066) 66: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None })
  R-00001: [000_000_066, 000_000_101) 35: Append((2, 7), "wow")
  R-00002: [000_000_101, 000_000_129) 28: PurgeUpto((2, 3))
ChunkId(00_000_000_000_000_000_638)
  R-00000: [000_000_000, 000_000_066) 66: State(RaftLogState { vote: None, last: Some((2, 7)), committed: Some((1, 2)), purged: Some((2, 3)), user_data: None })
"#},
                dump
            );
        }
    }

    Ok(())
}

#[test]
fn test_re_open_unfinished_tailing_not_all_zero_chunk() -> Result<(), io::Error>
{
    let append_zeros = 1024 * 32;

    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;
        sample_data::build_sample_data_purge_upto_3(&mut rl)?;
    }

    // Append several zero bytes followed by a one
    {
        let chunk_id = ChunkId(509);
        let mut f = Chunk::<TestTypes>::open_chunk_file(&ctx.config, chunk_id)?;
        f.set_len(129 + append_zeros)?;

        f.seek(io::SeekFrom::Start(129 + append_zeros))?;
        f.write_u8(1)?;
    }

    // Re-open
    {
        let res = ctx.new_raft_log();
        assert!(res.is_err());
        assert_eq!(
            "crc32 checksum mismatch: expected fd59b8d, got 0, \
            while Record::decode(); \
            when:(decode Record at offset 129); \
            when:(iterate ChunkId(00_000_000_000_000_000_509))",
            res.unwrap_err().to_string()
        );

        let dump =
            Dump::<TestTypes>::new(ctx.arc_config())?.write_to_string()?;
        println!("After reopen:\n{}", dump);

        assert_eq!(
            indoc! {r#"
RaftLog:
ChunkId(00_000_000_000_000_000_324)
  R-00000: [000_000_000, 000_000_050) 50: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None, user_data: None })
  R-00001: [000_000_050, 000_000_078) 28: PurgeUpto((1, 1))
  R-00002: [000_000_078, 000_000_115) 37: Append((2, 4), "world")
  R-00003: [000_000_115, 000_000_150) 35: Append((2, 5), "foo")
  R-00004: [000_000_150, 000_000_185) 35: Append((2, 6), "bar")
ChunkId(00_000_000_000_000_000_509)
  R-00000: [000_000_000, 000_000_066) 66: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None })
  R-00001: [000_000_066, 000_000_101) 35: Append((2, 7), "wow")
  R-00002: [000_000_101, 000_000_129) 28: PurgeUpto((2, 3))
Error: crc32 checksum mismatch: expected fd59b8d, got 0, while Record::decode(); when:(decode Record at offset 129); when:(iterate ChunkId(00_000_000_000_000_000_509))
"#},
            dump
        );
    }

    Ok(())
}

/// A damaged last record of non-last chunk will not be truncated, but is
/// considered damage.
#[test]
fn test_re_open_unfinished_non_last_chunk() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;
        sample_data::build_sample_data_purge_upto_3(&mut rl)?;
    }

    // Truncate the last record of the second last chunk,
    // the last record is at [148,183) size=35
    {
        let second_last_chunk_id = ChunkId(324);
        let f = Chunk::<TestTypes>::open_chunk_file(
            &ctx.config,
            second_last_chunk_id,
        )?;
        f.set_len(182)?;
    }

    // Re-open
    {
        let res = ctx.new_raft_log();
        assert!(res.is_err());
        // The last record of the second last chunk is damaged and is truncated.
        assert_eq!("Gap between chunks: 00_000_000_000_000_000_474 -> 00_000_000_000_000_000_509; Can not open, fix this error and re-open", res.unwrap_err().to_string());

        let dump =
            Dump::<TestTypes>::new(ctx.arc_config())?.write_to_string()?;
        println!("After reopen:\n{}", dump);

        assert_eq!(
            indoc! {r#"
RaftLog:
ChunkId(00_000_000_000_000_000_324)
  R-00000: [000_000_000, 000_000_050) 50: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None, user_data: None })
  R-00001: [000_000_050, 000_000_078) 28: PurgeUpto((1, 1))
  R-00002: [000_000_078, 000_000_115) 37: Append((2, 4), "world")
  R-00003: [000_000_115, 000_000_150) 35: Append((2, 5), "foo")
ChunkId(00_000_000_000_000_000_509)
  R-00000: [000_000_000, 000_000_066) 66: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None })
  R-00001: [000_000_066, 000_000_101) 35: Append((2, 7), "wow")
  R-00002: [000_000_101, 000_000_129) 28: PurgeUpto((2, 3))
"#},
            dump
        );
    }

    Ok(())
}

/// The last record is damaged, do not truncate, return an IO error.
#[test]
fn test_re_open_damaged_last_record() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;
        sample_data::build_sample_data_purge_upto_3(&mut rl)?;
    }

    // damage the last record, [99,127) size=28
    {
        let last_chunk_id = ChunkId(509);
        let mut f =
            Chunk::<TestTypes>::open_chunk_file(&ctx.config, last_chunk_id)?;

        f.seek(io::SeekFrom::Start(126))?;
        let byt = f.read_u8()?;
        let byt = byt.wrapping_add(1);
        f.seek(io::SeekFrom::Start(126))?;
        f.write_u8(byt)?;
    }

    // Re-open
    {
        let res = ctx.new_raft_log();
        assert!(res.is_err());
        assert_eq!(
            "crc32 checksum mismatch: expected cb22c57e, got cb23c57e, \
            while Record::decode(); \
            when:(decode Record at offset 101); \
            when:(iterate ChunkId(00_000_000_000_000_000_509))",
            res.unwrap_err().to_string()
        );

        let dump =
            Dump::<TestTypes>::new(ctx.arc_config())?.write_to_string()?;
        println!("After reopen:\n{}", dump);

        assert_eq!(
            indoc! {r#"
RaftLog:
ChunkId(00_000_000_000_000_000_324)
  R-00000: [000_000_000, 000_000_050) 50: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None, user_data: None })
  R-00001: [000_000_050, 000_000_078) 28: PurgeUpto((1, 1))
  R-00002: [000_000_078, 000_000_115) 37: Append((2, 4), "world")
  R-00003: [000_000_115, 000_000_150) 35: Append((2, 5), "foo")
  R-00004: [000_000_150, 000_000_185) 35: Append((2, 6), "bar")
ChunkId(00_000_000_000_000_000_509)
  R-00000: [000_000_000, 000_000_066) 66: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None })
  R-00001: [000_000_066, 000_000_101) 35: Append((2, 7), "wow")
Error: crc32 checksum mismatch: expected cb22c57e, got cb23c57e, while Record::decode(); when:(decode Record at offset 101); when:(iterate ChunkId(00_000_000_000_000_000_509))
"#},
            dump
        );
    }

    Ok(())
}
