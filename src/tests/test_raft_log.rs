use std::io;
use std::io::Seek;
use std::thread::sleep;
use std::time::Duration;

use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use codeq::Segment;
use indoc::indoc;
use pretty_assertions::assert_eq;

use crate::api::raft_log_writer::blocking_flush;
use crate::api::raft_log_writer::RaftLogWriter;
use crate::chunk::Chunk;
use crate::raft_log::dump::Dump;
use crate::raft_log::dump_api::DumpApi;
use crate::raft_log::raft_log::RaftLog;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;
use crate::testing::ss;
use crate::testing::TestTypes;
use crate::tests::test_context::new_testing;
use crate::tests::test_context::TestContext;
use crate::ChunkId;

#[test]
fn test_save_user_data() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    rl.save_user_data(Some(ss("foo")))?;

    let state = rl.log_state();
    assert_eq!(Some(ss("foo")), state.user_data);

    rl.save_user_data(None)?;

    let state = rl.log_state();
    assert_eq!(None, state.user_data);

    let want_dumped = indoc! {r#"
RaftLog:
ChunkId(00_000_000_000_000_000_000)
  R-00000: [000_000_000, 000_000_018) 18: State(RaftLogState { vote: None, last: None, committed: None, purged: None, user_data: None })
  R-00001: [000_000_018, 000_000_043) 25: State(RaftLogState { vote: None, last: None, committed: None, purged: None, user_data: Some("foo") })
  R-00002: [000_000_043, 000_000_061) 18: State(RaftLogState { vote: None, last: None, committed: None, purged: None, user_data: None })
"#};

    let dump = rl.dump().write_to_string()?;
    println!("{}", dump);
    assert_eq!(want_dumped, dump);

    Ok(())
}

#[test]
fn test_save_vote() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    let vote = (1, 1);
    rl.save_vote(vote)?;

    let state = rl.log_state();
    assert_eq!(Some(vote), state.vote);

    Ok(())
}

/// Open an empty RaftLog and append and read entries.
#[test]
fn test_append_and_read() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState::default());

    let logs = [
        //
        ((1, 0), ss("hello")),
        ((1, 1), ss("world")),
    ];

    let seg = rl.append(logs.clone())?;
    assert_eq!(Segment::new(55, 37), seg);

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((1, 1)),
        ..RaftLogState::default()
    });

    let got = rl.read(0, 2).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(logs.to_vec(), got);

    Ok(())
}

#[test]
fn test_read_nonexistent_logs() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    // Update `last` to allow to append at index 2
    let state = rl.log_state_mut();
    state.last = Some((1, 1));

    let logs = [
        //
        ((2, 2), ss("hello")),
        ((3, 3), ss("world")),
    ];

    rl.append(logs.clone())?;

    let got = rl.read(0, 5).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(logs.to_vec(), got);

    Ok(())
}

#[test]
fn test_truncate() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    let logs = [
        //
        ((1, 0), ss("hello")),
        ((1, 1), ss("world")),
        ((1, 2), ss("foo")),
        ((1, 3), ss("bar")),
    ];

    rl.append(logs.clone())?;

    let seg = rl.truncate(2)?;
    assert_eq!(Segment::new(162, 29), seg);

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((1, 1)),
        ..RaftLogState::default()
    });

    let got = rl.read(0, 5).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(logs[..2].to_vec(), got);

    Ok(())
}

#[test]
fn test_truncate_just_after_purged() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    let state = rl.log_state_mut();
    state.purged = Some((1, 1));

    let logs = [
        //
        ((1, 0), ss("hello")),
        ((1, 1), ss("world")),
        ((1, 2), ss("foo")),
        ((1, 3), ss("bar")),
    ];

    rl.append(logs.clone())?;

    let seg = rl.truncate(2)?;
    assert_eq!(Segment::new(162, 29), seg);

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((1, 1)),
        purged: Some((1, 1)),
        ..RaftLogState::default()
    });

    let got = rl.read(0, 5).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(logs[..2].to_vec(), got);

    Ok(())
}

#[test]
fn test_truncate_non_existent() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    let state = rl.log_state_mut();
    state.last = Some((1, 1));

    let logs = [
        //
        ((1, 2), ss("world")),
        ((1, 3), ss("foo")),
    ];

    rl.append(logs.clone())?;

    assert!(
        rl.truncate(4).is_ok(),
        "truncate after last log should be ok"
    );

    assert!(rl.truncate(5).is_err(),);
    assert!(rl.truncate(1).is_err(),);

    Ok(())
}

#[test]
fn test_purge() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    let state = rl.log_state_mut();
    state.purged = Some((1, 0));
    state.last = Some((1, 0));

    let logs = [
        //
        ((1, 1), ss("hello")),
        ((1, 2), ss("world")),
        ((1, 3), ss("foo")),
    ];

    rl.append(logs.clone())?;

    // Purge at current purged

    let seg = rl.purge((1, 0))?;
    blocking_flush(&mut rl)?;
    assert_eq!(Segment::new(92, 35), seg);

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((1, 3)),
        purged: Some((1, 0)),
        ..RaftLogState::default()
    });

    let got = rl.read(0, 5).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(logs.to_vec(), got);

    //

    rl.purge((1, 2))?;
    blocking_flush(&mut rl)?;

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((1, 3)),
        purged: Some((1, 2)),
        ..RaftLogState::default()
    });

    let got = rl.read(0, 5).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(logs[2..=2].to_vec(), got);

    // Purge before last purged

    rl.purge((1, 1))?;
    blocking_flush(&mut rl)?;

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((1, 3)),
        purged: Some((1, 2)),
        ..RaftLogState::default()
    });

    let got = rl.read(0, 5).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(logs[2..=2].to_vec(), got);

    // Purge advance last

    rl.purge((2, 4))?;
    blocking_flush(&mut rl)?;

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((2, 4)),
        purged: Some((2, 4)),
        ..RaftLogState::default()
    });

    let got = rl.read(0, 5).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(got, vec![]);

    Ok(())
}

/// When reopened, the purge state should be restored
#[test]
fn test_purge_reopen() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;

        let logs = [
            //
            ((1, 0), ss("hi")),
            ((1, 1), ss("hello")),
            ((1, 2), ss("world")),
            ((1, 3), ss("foo")),
            ((1, 4), ss("bar")),
            ((1, 5), ss("wow")),
            ((1, 6), ss("biz")),
        ];
        rl.append(logs)?;

        rl.purge((1, 5))?;
        blocking_flush(&mut rl)?;
    }
    {
        let rl = ctx.new_raft_log()?;

        let logs = [
            //
            ((1, 6), ss("biz")),
        ];

        let got = rl.read(0, 10).collect::<Result<Vec<_>, io::Error>>()?;
        assert_eq!(logs.to_vec(), got);
    }

    Ok(())
}

#[test]
fn test_commit() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    let state = rl.log_state_mut();
    state.last = Some((1, 1));

    let logs = [
        //
        ((1, 2), ss("hello")),
        ((1, 3), ss("world")),
        ((1, 4), ss("foo")),
    ];

    rl.append(logs.clone())?;

    let seg = rl.commit((1, 3))?;
    assert_eq!(Segment::new(127, 28), seg);

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((1, 4)),
        committed: Some((1, 3)),
        ..RaftLogState::default()
    });

    // Update commit

    assert!(rl.commit((1, 2)).is_err());
    rl.commit((1, 4))?;

    assert_eq!(rl.log_state().committed, Some((1, 4)));

    Ok(())
}

#[test]
fn test_purge_removes_chunks() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;

        let want_dumped = build_sample_data(&mut rl)?;

        let dump = rl.dump().write_to_string()?;
        println!("Before purge:\n{}", dump);
        assert_eq!(want_dumped, dump);

        rl.purge((2, 3))?;
        blocking_flush(&mut rl)?;

        sleep(Duration::from_secs(1));

        let dump = rl.dump().write_to_string()?;
        println!("After purge:\n{}", dump);
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
            "#},
            dump
        );
    }

    Ok(())
}

#[test]
fn test_re_open() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);

    let (state, logs) = {
        let mut rl = ctx.new_raft_log()?;
        build_sample_data_purge_upto_3(&mut rl)?;

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
        build_sample_data_purge_upto_3(&mut rl)?;

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

/// A damaged last record of non-last chunk will not be truncated, but is
/// considered a damage.
#[test]
fn test_re_open_unfinished_non_last_chunk() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;
        build_sample_data_purge_upto_3(&mut rl)?;
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
        build_sample_data_purge_upto_3(&mut rl)?;
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

#[test]
fn test_read_with_cache() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);
    config.log_cache_max_items = Some(3);

    {
        let mut rl = ctx.new_raft_log()?;
        build_sample_data_purge_upto_3(&mut rl)?;

        rl.read(4, 8).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            "AccessStat{cache(hit/miss)=4/0}",
            rl.access_stat().to_string(),
            "evitable cursor is updated but not yet evicted by next insert"
        );

        rl.read(5, 8).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            "AccessStat{cache(hit/miss)=7/0}",
            rl.access_stat().to_string()
        );

        let logs = [
            //
            ((2, 8), ss("biubiu")),
        ];
        rl.append(logs)?;

        rl.read(5, 9).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            "AccessStat{cache(hit/miss)=10/1}",
            rl.access_stat().to_string(),
            "last insert evicts item"
        );
    }

    // Re-open
    {
        let mut rl = ctx.new_raft_log()?;
        let logs = [
            //
            ((3, 9), ss("goo")),
        ];
        rl.append(logs)?;

        println!("After re-open:\n{}", rl.dump().write_to_string()?);

        // present logs: [4, 9): not in cache: 45; in cache: 678

        rl.read(4, 10).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            "AccessStat{cache(hit/miss)=3/3}",
            rl.access_stat().to_string()
        );

        rl.read(5, 9).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            "AccessStat{cache(hit/miss)=5/5}",
            rl.access_stat().to_string()
        );
    }

    Ok(())
}

/// This test ensures that the logs in the open chunk are always cached.
#[test]
fn test_read_without_cache() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);
    config.log_cache_capacity = Some(0);

    {
        let mut rl = ctx.new_raft_log()?;
        build_sample_data_purge_upto_3(&mut rl)?;

        // Insert to trigger evict
        let logs = [
            //
            ((2, 8), ss("world")),
        ];
        rl.append(logs)?;

        println!("after insert:\n{}", rl.dump().write_to_string()?);

        let got = rl.read(0, 1000).collect::<Result<Vec<_>, _>>()?;

        let logs = [
            //
            ((2, 4), ss("world")),
            ((2, 5), ss("foo")),
            ((2, 6), ss("bar")),
            ((2, 7), ss("wow")),
            ((2, 8), ss("world")),
        ];
        assert_eq!(logs.to_vec(), got);
        assert_eq!(
            "AccessStat{cache(hit/miss)=2/3}",
            rl.access_stat().to_string(),
            "logs in open chunk are always cached"
        );
    }

    // Re-open
    {
        let mut rl = ctx.new_raft_log()?;

        println!("re-open:\n{}", rl.dump().write_to_string()?);
        let got = rl.read(0, 1000).collect::<Result<Vec<_>, _>>()?;
        let logs = [
            //
            ((2, 4), ss("world")),
            ((2, 5), ss("foo")),
            ((2, 6), ss("bar")),
            ((2, 7), ss("wow")),
            ((2, 8), ss("world")),
        ];
        assert_eq!(logs.to_vec(), got);
        assert_eq!(
            "AccessStat{cache(hit/miss)=2/3}",
            rl.access_stat().to_string(),
            "the hit is 2 because the last inserted log has not yet evicted by a new insert, logs in the last closed chunk is still cached"
        );

        let logs = [
            //
            ((3, 9), ss("goo")),
        ];
        rl.append(logs)?;
        println!("After append:\n{}", rl.dump().write_to_string()?);

        let got = rl.read(0, 1000).collect::<Result<Vec<_>, _>>()?;

        let logs = [
            //
            ((2, 4), ss("world")),
            ((2, 5), ss("foo")),
            ((2, 6), ss("bar")),
            ((2, 7), ss("wow")),
            ((2, 8), ss("world")),
            ((3, 9), ss("goo")),
        ];
        assert_eq!(logs.to_vec(), got);
        assert_eq!(
            "AccessStat{cache(hit/miss)=3/8}",
            rl.access_stat().to_string(),
            "another hit on the last log"
        );
    }

    Ok(())
}

/// Inside RaftLog, read and append use the same `File` instance, and call
/// `seek` on it. This test ensure the `seek` does not affect each other.
#[test]
fn test_read_does_not_affect_append() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);
    // Turn off log cache so that every read seeks.
    config.log_cache_capacity = Some(0);

    {
        let mut rl = ctx.new_raft_log()?;

        rl.append([((1, 0), ss("hi"))])?;
        let got = rl.read(0, 1).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(vec![((1, 0), ss("hi"))], got);

        rl.append([((1, 1), ss("hello"))])?;
        let got = rl.read(0, 1).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(vec![((1, 0), ss("hi"))], got);

        rl.append([((1, 2), ss("world"))])?;
        let got = rl.read(0, 1).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(vec![((1, 0), ss("hi"))], got);

        assert_eq!(
            "AccessStat{cache(hit/miss)=3/0}",
            rl.access_stat().to_string(),
            "all logs in open chunk are cached"
        );
    }

    Ok(())
}

#[test]
fn test_sync() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;

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
        let logs = [
            //
            ((2, 4), ss("world")),
            ((2, 5), ss("foo")),
            ((2, 6), ss("bar")),
            ((2, 7), ss("wow")),
        ];
        rl.append(logs)?;

        let flush_stat = rl.wal.get_stat()?;
        assert_eq!(
            vec![(0, 0,), (161, 0,), (324, 0,), (509, 0)],
            flush_stat,
            "no synced"
        );

        blocking_flush(&mut rl)?;

        let flush_stat = rl.wal.get_stat()?;
        assert_eq!(
            vec![(509, 610)],
            flush_stat,
            "4 chunks, all synced, the first 3 are removed"
        );

        let logs = [
            //
            ((2, 8), ss("world")),
        ];

        rl.append(logs)?;
        blocking_flush(&mut rl)?;

        let flush_stat = rl.wal.get_stat()?;
        assert_eq!(
            vec![(509, 647)],
            flush_stat,
            "4 chunks, all synced, the first 3 are removed"
        );
    }

    Ok(())
}

#[test]
fn test_on_disk_size() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);
    config.log_cache_capacity = Some(0);

    {
        let mut rl = ctx.new_raft_log()?;
        build_sample_data_purge_upto_3(&mut rl)?;
        assert_eq!(rl.on_disk_size(), 314);
    }
    Ok(())
}

#[test]
fn test_update_state() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.chunk_max_records = Some(5);
    config.log_cache_capacity = Some(0);

    {
        let mut rl = ctx.new_raft_log()?;
        build_sample_data_purge_upto_3(&mut rl)?;
        rl.update_state(RaftLogState {
            vote: Some((1, 2)),
            last: Some((3, 4)),
            committed: None,
            purged: None,
            user_data: None,
        })?;

        assert_eq!(rl.log_state(), &RaftLogState {
            vote: Some((1, 2)),
            last: Some((3, 4)),
            committed: None,
            purged: None,
            user_data: None,
        });

        blocking_flush(&mut rl)?;

        let dump = rl.dump().write_to_string()?;
        println!("{}", dump);
    }

    {
        let rl = ctx.new_raft_log()?;
        assert_eq!(rl.log_state(), &RaftLogState {
            vote: Some((1, 2)),
            last: Some((3, 4)),
            committed: None,
            purged: None,
            user_data: None,
        });
    }
    Ok(())
}

fn build_sample_data_purge_upto_3(
    rl: &mut RaftLog<TestTypes>,
) -> Result<String, io::Error> {
    build_sample_data(rl)?;

    rl.purge((2, 3))?;
    blocking_flush(rl)?;

    let dumped = indoc! {r#"
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
        "#};

    let dump = rl.dump().write_to_string()?;
    println!("After purge:\n{}", dump);

    assert_eq!(dumped, dump);

    // Wait for FlushWorker to quit and remove purged chunks
    sleep(Duration::from_millis(100));

    Ok(dumped.to_string())
}

fn build_sample_data(rl: &mut RaftLog<TestTypes>) -> Result<String, io::Error> {
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
