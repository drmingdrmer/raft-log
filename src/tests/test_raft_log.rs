use std::io;
use std::io::Seek;
use std::sync::Arc;

use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use codeq::Segment;
use indoc::indoc;
use pretty_assertions::assert_eq;

use crate::api::raft_log_writer::RaftLogWriter;
use crate::chunk::Chunk;
use crate::raft_log::raft_log::RaftLog;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;
use crate::testing::ss;
use crate::testing::TestTypes;
use crate::tests::test_context::new_testing;
use crate::tests::test_context::TestContext;
use crate::ChunkId;

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
    assert_eq!(Segment::new(53, 37), seg);

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
    assert_eq!(Segment::new(160, 29), seg);

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
    assert_eq!(Segment::new(160, 29), seg);

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

    let seg = rl.purge(0)?;
    assert_eq!(Segment::new(90, 35), seg);

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((1, 3)),
        purged: Some((1, 0)),
        ..RaftLogState::default()
    });

    let got = rl.read(0, 5).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(logs.to_vec(), got);

    //

    rl.purge(2)?;

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((1, 3)),
        purged: Some((1, 2)),
        ..RaftLogState::default()
    });

    let got = rl.read(0, 5).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(logs[2..=2].to_vec(), got);

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
    assert_eq!(Segment::new(125, 28), seg);

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

    config.max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;

        let want_dumped = build_sample_data(&mut rl)?;

        let dump =
            RaftLog::<TestTypes>::dump(ctx.arc_config()).write_to_string()?;
        println!("Before purge:\n{}", dump);
        assert_eq!(want_dumped, dump);

        rl.purge(3)?;

        let dump =
            RaftLog::<TestTypes>::dump(ctx.arc_config()).write_to_string()?;
        println!("After purge:\n{}", dump);
        assert_eq!(
            indoc! {r#"
            RaftLog:
            ChunkId(00_000_000_000_000_000_320)
              R-00000: [000_000_000, 000_000_048) 48: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None })
              R-00001: [000_000_048, 000_000_076) 28: PurgeUpto((1, 1))
              R-00002: [000_000_076, 000_000_113) 37: Append((2, 4), "world")
              R-00003: [000_000_113, 000_000_148) 35: Append((2, 5), "foo")
              R-00004: [000_000_148, 000_000_183) 35: Append((2, 6), "bar")
            ChunkId(00_000_000_000_000_000_503)
              R-00000: [000_000_000, 000_000_064) 64: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)) })
              R-00001: [000_000_064, 000_000_099) 35: Append((2, 7), "wow")
              R-00002: [000_000_099, 000_000_127) 28: PurgeUpto((2, 3))
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

    config.max_records = Some(5);

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

        let dump =
            RaftLog::<TestTypes>::dump(ctx.arc_config()).write_to_string()?;
        println!("After reopen:\n{}", dump);

        assert_eq!(
            indoc! {r#"
            RaftLog:
            ChunkId(00_000_000_000_000_000_320)
              R-00000: [000_000_000, 000_000_048) 48: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None })
              R-00001: [000_000_048, 000_000_076) 28: PurgeUpto((1, 1))
              R-00002: [000_000_076, 000_000_113) 37: Append((2, 4), "world")
              R-00003: [000_000_113, 000_000_148) 35: Append((2, 5), "foo")
              R-00004: [000_000_148, 000_000_183) 35: Append((2, 6), "bar")
            ChunkId(00_000_000_000_000_000_503)
              R-00000: [000_000_000, 000_000_064) 64: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)) })
              R-00001: [000_000_064, 000_000_099) 35: Append((2, 7), "wow")
              R-00002: [000_000_099, 000_000_127) 28: PurgeUpto((2, 3))
            ChunkId(00_000_000_000_000_000_630)
              R-00000: [000_000_000, 000_000_064) 64: State(RaftLogState { vote: None, last: Some((2, 7)), committed: Some((1, 2)), purged: Some((2, 3)) })
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

    config.max_records = Some(5);

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
        let chunk_id = ChunkId(503);
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

        let dump =
            RaftLog::<TestTypes>::dump(ctx.arc_config()).write_to_string()?;
        println!("After reopen:\n{}", dump);

        assert_eq!(
            indoc! {r#"
            RaftLog:
            ChunkId(00_000_000_000_000_000_320)
              R-00000: [000_000_000, 000_000_048) 48: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None })
              R-00001: [000_000_048, 000_000_076) 28: PurgeUpto((1, 1))
              R-00002: [000_000_076, 000_000_113) 37: Append((2, 4), "world")
              R-00003: [000_000_113, 000_000_148) 35: Append((2, 5), "foo")
              R-00004: [000_000_148, 000_000_183) 35: Append((2, 6), "bar")
            ChunkId(00_000_000_000_000_000_503)
              R-00000: [000_000_000, 000_000_064) 64: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)) })
              R-00001: [000_000_064, 000_000_099) 35: Append((2, 7), "wow")
            ChunkId(00_000_000_000_000_000_602)
              R-00000: [000_000_000, 000_000_064) 64: State(RaftLogState { vote: None, last: Some((2, 7)), committed: Some((1, 2)), purged: Some((1, 1)) })
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

    config.max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;
        build_sample_data_purge_upto_3(&mut rl)?;
    }

    // Truncate the last record of the second last chunk,
    // the last record is at [148,183) size=35
    {
        let second_last_chunk_id = ChunkId(320);
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
        assert_eq!("Gap between chunks: 00_000_000_000_000_000_468 -> 00_000_000_000_000_000_503; Can not open, fix this error and re-open", res.unwrap_err().to_string());

        let dump =
            RaftLog::<TestTypes>::dump(ctx.arc_config()).write_to_string()?;
        println!("After reopen:\n{}", dump);

        assert_eq!(
            indoc! {r#"
            RaftLog:
            ChunkId(00_000_000_000_000_000_320)
              R-00000: [000_000_000, 000_000_048) 48: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None })
              R-00001: [000_000_048, 000_000_076) 28: PurgeUpto((1, 1))
              R-00002: [000_000_076, 000_000_113) 37: Append((2, 4), "world")
              R-00003: [000_000_113, 000_000_148) 35: Append((2, 5), "foo")
            ChunkId(00_000_000_000_000_000_503)
              R-00000: [000_000_000, 000_000_064) 64: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)) })
              R-00001: [000_000_064, 000_000_099) 35: Append((2, 7), "wow")
              R-00002: [000_000_099, 000_000_127) 28: PurgeUpto((2, 3))
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

    config.max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;
        build_sample_data_purge_upto_3(&mut rl)?;
    }

    // damage the last record, [99,127) size=28
    {
        let last_chunk_id = ChunkId(503);
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
            "crc32 checksum mismatch: expected cb22c57e, got cb22c57f, \
            while Record::decode(); \
            when:(decode Record at offset 99); \
            when:(iterate ChunkId(00_000_000_000_000_000_503))",
            res.unwrap_err().to_string()
        );

        let dump =
            RaftLog::<TestTypes>::dump(ctx.arc_config()).write_to_string()?;
        println!("After reopen:\n{}", dump);

        assert_eq!(
            indoc! {r#"
            RaftLog:
            ChunkId(00_000_000_000_000_000_320)
              R-00000: [000_000_000, 000_000_048) 48: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None })
              R-00001: [000_000_048, 000_000_076) 28: PurgeUpto((1, 1))
              R-00002: [000_000_076, 000_000_113) 37: Append((2, 4), "world")
              R-00003: [000_000_113, 000_000_148) 35: Append((2, 5), "foo")
              R-00004: [000_000_148, 000_000_183) 35: Append((2, 6), "bar")
            ChunkId(00_000_000_000_000_000_503)
              R-00000: [000_000_000, 000_000_064) 64: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)) })
              R-00001: [000_000_064, 000_000_099) 35: Append((2, 7), "wow")
            Error: crc32 checksum mismatch: expected cb22c57e, got cb22c57f, while Record::decode(); when:(decode Record at offset 99); when:(iterate ChunkId(00_000_000_000_000_000_503))
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

    config.max_records = Some(5);
    config.log_cache_max_items = Some(3);

    {
        let mut rl = ctx.new_raft_log()?;
        build_sample_data_purge_upto_3(&mut rl)?;

        rl.read(4, 8).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            "AccessStat{cache(hit/miss)=3/1}",
            rl.access_stat().to_string()
        );

        rl.read(5, 8).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            "AccessStat{cache(hit/miss)=6/1}",
            rl.access_stat().to_string()
        );
    }

    // Re-open
    {
        let mut rl = ctx.new_raft_log()?;
        let logs = [
            //
            ((3, 8), ss("goo")),
        ];
        rl.append(logs)?;

        // present logs: [4, 9): not in cache: 45; in cache: 678

        rl.read(4, 9).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            "AccessStat{cache(hit/miss)=3/2}",
            rl.access_stat().to_string()
        );

        rl.read(5, 8).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            "AccessStat{cache(hit/miss)=5/3}",
            rl.access_stat().to_string()
        );
    }

    Ok(())
}

#[test]
fn test_read_without_cache() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.max_records = Some(5);
    config.log_cache_capacity = Some(0);

    {
        let mut rl = ctx.new_raft_log()?;
        build_sample_data_purge_upto_3(&mut rl)?;

        let got = rl.read(0, 1000).collect::<Result<Vec<_>, _>>()?;

        let logs = [
            //
            ((2, 4), ss("world")),
            ((2, 5), ss("foo")),
            ((2, 6), ss("bar")),
            ((2, 7), ss("wow")),
        ];
        assert_eq!(logs.to_vec(), got);
        assert_eq!(
            "AccessStat{cache(hit/miss)=0/4}",
            rl.access_stat().to_string()
        );
    }

    // Re-open
    {
        let mut rl = ctx.new_raft_log()?;
        let logs = [
            //
            ((3, 8), ss("goo")),
        ];
        rl.append(logs)?;

        let got = rl.read(0, 1000).collect::<Result<Vec<_>, _>>()?;

        let logs = [
            //
            ((2, 4), ss("world")),
            ((2, 5), ss("foo")),
            ((2, 6), ss("bar")),
            ((2, 7), ss("wow")),
            ((3, 8), ss("goo")),
        ];
        assert_eq!(logs.to_vec(), got);
        assert_eq!(
            "AccessStat{cache(hit/miss)=0/5}",
            rl.access_stat().to_string()
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

    config.max_records = Some(5);
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
            "AccessStat{cache(hit/miss)=0/3}",
            rl.access_stat().to_string()
        );
    }

    Ok(())
}

#[test]
fn test_sync() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;

        build_sample_data(&mut rl)?;

        let sync_stat = rl.wal.get_stat()?;
        assert_eq!(
            vec![(0, 0), (159, 0), (320, 0), (503, 0)],
            sync_stat,
            "4 chunks(starting offset and synced-offset), all unsynced"
        );

        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        rl.flush(tx)?;
        rx.recv().unwrap()?;

        let sync_stat = rl.wal.get_stat()?;
        assert_eq!(
            vec![(503, 602)],
            sync_stat,
            "4 chunks, all synced, the first 3 are removed"
        );
    }

    Ok(())
}

fn build_sample_data_purge_upto_3(
    rl: &mut RaftLog<TestTypes>,
) -> Result<String, io::Error> {
    build_sample_data(rl)?;
    rl.purge(3)?;

    let dumped = indoc! {r#"
        RaftLog:
        ChunkId(00_000_000_000_000_000_320)
          R-00000: [000_000_000, 000_000_048) 48: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None })
          R-00001: [000_000_048, 000_000_076) 28: PurgeUpto((1, 1))
          R-00002: [000_000_076, 000_000_113) 37: Append((2, 4), "world")
          R-00003: [000_000_113, 000_000_148) 35: Append((2, 5), "foo")
          R-00004: [000_000_148, 000_000_183) 35: Append((2, 6), "bar")
        ChunkId(00_000_000_000_000_000_503)
          R-00000: [000_000_000, 000_000_064) 64: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)) })
          R-00001: [000_000_064, 000_000_099) 35: Append((2, 7), "wow")
          R-00002: [000_000_099, 000_000_127) 28: PurgeUpto((2, 3))
        "#};

    let dump = RaftLog::<TestTypes>::dump(Arc::new(rl.config().clone()))
        .write_to_string()?;
    println!("After purge:\n{}", dump);

    assert_eq!(dumped, dump);

    Ok(dumped.to_string())
}

fn build_sample_data(rl: &mut RaftLog<TestTypes>) -> Result<String, io::Error> {
    assert_eq!(rl.config.max_records, Some(5));

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
    rl.purge(1)?;

    let logs = [
        //
        ((2, 4), ss("world")),
        ((2, 5), ss("foo")),
        ((2, 6), ss("bar")),
        ((2, 7), ss("wow")),
    ];
    rl.append(logs)?;

    let dumped = indoc! {r#"
        RaftLog:
        ChunkId(00_000_000_000_000_000_000)
          R-00000: [000_000_000, 000_000_016) 16: State(RaftLogState { vote: None, last: None, committed: None, purged: None })
          R-00001: [000_000_016, 000_000_050) 34: Append((1, 0), "hi")
          R-00002: [000_000_050, 000_000_087) 37: Append((1, 1), "hello")
          R-00003: [000_000_087, 000_000_124) 37: Append((1, 2), "world")
          R-00004: [000_000_124, 000_000_159) 35: Append((1, 3), "foo")
        ChunkId(00_000_000_000_000_000_159)
          R-00000: [000_000_000, 000_000_032) 32: State(RaftLogState { vote: None, last: Some((1, 3)), committed: None, purged: None })
          R-00001: [000_000_032, 000_000_061) 29: TruncateAfter(Some((1, 1)))
          R-00002: [000_000_061, 000_000_098) 37: Append((2, 2), "world")
          R-00003: [000_000_098, 000_000_133) 35: Append((2, 3), "foo")
          R-00004: [000_000_133, 000_000_161) 28: Commit((1, 2))
        ChunkId(00_000_000_000_000_000_320)
          R-00000: [000_000_000, 000_000_048) 48: State(RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None })
          R-00001: [000_000_048, 000_000_076) 28: PurgeUpto((1, 1))
          R-00002: [000_000_076, 000_000_113) 37: Append((2, 4), "world")
          R-00003: [000_000_113, 000_000_148) 35: Append((2, 5), "foo")
          R-00004: [000_000_148, 000_000_183) 35: Append((2, 6), "bar")
        ChunkId(00_000_000_000_000_000_503)
          R-00000: [000_000_000, 000_000_064) 64: State(RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)) })
          R-00001: [000_000_064, 000_000_099) 35: Append((2, 7), "wow")
        "#};
    Ok(dumped.to_string())
}
