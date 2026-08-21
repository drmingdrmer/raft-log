use std::io;
use std::sync::mpsc::sync_channel;
use std::time::Duration;

use chunked_wal::wal::FlushStat;
use indoc::indoc;
use pretty_assertions::assert_eq;

use crate::Dump;
use crate::api::raft_log_writer::RaftLogWriter;
use crate::api::raft_log_writer::blocking_flush;
use crate::raft_log::dump_api::DumpApi;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;
use crate::testing::TestTypes;
use crate::testing::ss;
use crate::tests::context::TestContext;
use crate::tests::context::new_testing;
use crate::tests::sample_data;
use crate::tests::sample_data::build_sample_data;
use crate::types::Segment;

#[test]
fn test_save_user_data() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    rl.save_user_data(Some(ss("foo")))?;

    let state = rl.log_state();
    assert_eq!(Some(ss("foo")), state.user_data);

    rl.save_user_data(None)?;

    let state = rl.log_state();
    assert_eq!(None, state.user_data);

    blocking_flush(&mut rl)?;

    let want_dumped = indoc! {r#"
RaftLog:
ChunkId(00_000_000_000_000_000_000)
  R-00000: [000_000_000, 000_000_018) Size(18): State(RaftLogState { vote: None, last: None, committed: None, purged: None, user_data: None })
  R-00001: [000_000_018, 000_000_043) Size(25): State(RaftLogState { vote: None, last: None, committed: None, purged: None, user_data: Some("foo") })
  R-00002: [000_000_043, 000_000_061) Size(18): State(RaftLogState { vote: None, last: None, committed: None, purged: None, user_data: None })
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

#[test]
fn test_open_holds_wal_lock() -> Result<(), io::Error> {
    let ctx = TestContext::new()?;
    let rl = ctx.new_raft_log()?;

    let err = match ctx.new_raft_log() {
        Ok(_) => panic!("opening the same RaftLog twice must fail"),
        Err(err) => err,
    };
    assert_eq!(io::ErrorKind::WouldBlock, err.kind());

    drop(rl);

    let _rl = ctx.new_raft_log()?;

    Ok(())
}

#[test]
fn test_dump_new_requires_wal_lock_but_ref_dump_does_not()
-> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    ctx.config.wal.chunk_max_records = Some(5);

    let expected = {
        let mut rl = ctx.new_raft_log()?;
        let expected = build_sample_data(&mut rl)?;

        assert_eq!(expected, rl.dump().write_to_string()?);

        let err = match Dump::<TestTypes>::new(ctx.arc_config()) {
            Ok(_) => {
                panic!("Dump::new must fail while RaftLog owns the WAL lock")
            }
            Err(err) => err,
        };
        assert_eq!(io::ErrorKind::WouldBlock, err.kind());

        expected
    };

    assert_eq!(
        expected,
        Dump::<TestTypes>::new(ctx.arc_config())?.write_to_string()?
    );

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

/// `truncate(0)` on a log whose entries start above index 0 must return an
/// error. The index it would keep is already purged, and computing it used to
/// underflow: a debug build panicked, a release build wrapped to `u64::MAX`.
#[test]
fn test_truncate_zero_below_purged() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    rl.append([((1, 0), ss("hello")), ((1, 1), ss("world"))])?;
    rl.purge((1, 0))?;

    let err = rl.truncate(0).unwrap_err();
    assert_eq!("Log not found at index 0", err.to_string());

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((1, 1)),
        purged: Some((1, 0)),
        ..RaftLogState::default()
    });

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

    let seg = rl.purge((1, 1))?;
    blocking_flush(&mut rl)?;
    assert_eq!(Segment::new(127, 28), seg);

    let state = rl.log_state();
    assert_eq!(state, &RaftLogState {
        last: Some((1, 3)),
        purged: Some((1, 1)),
        ..RaftLogState::default()
    });

    let got = rl.read(0, 5).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(logs[1..=2].to_vec(), got);

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

/// A rejected write must change nothing and must leave the store reopenable.
///
/// The record is checked before `WAL::append` puts it in the open chunk's
/// pending buffer. Without that order the rejected record still reaches the
/// buffer, the next flush makes it durable, and `RaftLog::open` then replays it
/// and fails with the very error that rejected it.
#[test]
fn test_rejected_write_leaves_log_unchanged() -> Result<(), io::Error> {
    let ctx = TestContext::new()?;

    let logs = [((1, 0), ss("hi")), ((1, 1), ss("hello"))];

    {
        let mut rl = ctx.new_raft_log()?;
        rl.append(logs.clone())?;
        rl.save_vote((3, 3))?;
        rl.commit((1, 1))?;

        // Index 1 is taken, so this append is a log id reversal.
        let err = rl.append([((1, 1), ss("poison"))]).unwrap_err();
        let want = "Log id cannot be reversed when append: current (1, 1), attempted (1, 1)";
        assert_eq!(want, err.to_string());

        let err = rl.save_vote((2, 2)).unwrap_err();
        let want = "Vote cannot be reversed: current (3, 3), attempted (2, 2)";
        assert_eq!(want, err.to_string());

        let err = rl.commit((1, 0)).unwrap_err();
        let want = "Log id cannot be reversed when commit: current (1, 1), attempted (1, 0)";
        assert_eq!(want, err.to_string());

        let got = rl.read(0, 10).collect::<Result<Vec<_>, io::Error>>()?;
        assert_eq!(logs.to_vec(), got);

        let state = rl.log_state();
        assert_eq!(state, &RaftLogState {
            vote: Some((3, 3)),
            last: Some((1, 1)),
            committed: Some((1, 1)),
            ..RaftLogState::default()
        });

        blocking_flush(&mut rl)?;
    }
    {
        let rl = ctx.new_raft_log()?;

        let state = rl.log_state();
        assert_eq!(state, &RaftLogState {
            vote: Some((3, 3)),
            last: Some((1, 1)),
            committed: Some((1, 1)),
            ..RaftLogState::default()
        });

        let got = rl.read(0, 10).collect::<Result<Vec<_>, io::Error>>()?;
        assert_eq!(logs.to_vec(), got);
    }

    Ok(())
}

/// A purge boundary must name the log id that is already stored at its index.
///
/// The rejected call must also leave the log untouched and usable: the check
/// runs before the record enters the WAL buffer, so nothing is left behind for
/// a later flush to make durable.
#[test]
fn test_purge_rejects_log_id_conflicting_with_stored_log()
-> Result<(), io::Error> {
    let ctx = TestContext::new()?;

    let logs = [
        //
        ((1, 0), ss("hi")),
        ((1, 1), ss("hello")),
        ((2, 2), ss("world")),
        ((2, 3), ss("foo")),
    ];

    {
        let mut rl = ctx.new_raft_log()?;
        rl.append(logs.clone())?;

        // Index 1 stores `(1, 1)`, so term 3 at that index is not this log.
        let err = rl.purge((3, 1)).unwrap_err();
        let want = "Log id conflicts with the stored log id when purge: stored (1, 1), attempted (3, 1); log id order and log index order must agree";
        assert_eq!(want, err.to_string());

        // Index 2 stores `(2, 2)`. This boundary is below `last` by log id and
        // by index alike, so only the stored log id can reject it.
        let err = rl.purge((1, 2)).unwrap_err();
        let want = "Log id conflicts with the stored log id when purge: stored (2, 2), attempted (1, 2); log id order and log index order must agree";
        assert_eq!(want, err.to_string());

        let state = rl.log_state();
        assert_eq!(state, &RaftLogState {
            last: Some((2, 3)),
            ..RaftLogState::default()
        });

        rl.purge((2, 2))?;
        blocking_flush(&mut rl)?;
    }
    {
        let rl = ctx.new_raft_log()?;

        let state = rl.log_state();
        assert_eq!(state, &RaftLogState {
            last: Some((2, 3)),
            purged: Some((2, 2)),
            ..RaftLogState::default()
        });

        let got = rl.read(0, 10).collect::<Result<Vec<_>, io::Error>>()?;
        assert_eq!(logs[3..=3].to_vec(), got);
    }

    Ok(())
}

/// When reopened, the purge state should be restored
#[test]
fn test_purge_reopen() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.wal.chunk_max_records = Some(5);

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

    config.wal.chunk_max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;

        let want_dumped = sample_data::build_sample_data(&mut rl)?;

        let dump = rl.dump().write_to_string()?;
        println!("Before purge:\n{}", dump);
        assert_eq!(want_dumped, dump);

        rl.purge((2, 3))?;
        blocking_flush(&mut rl)?;

        rl.wait_worker_idle()?;

        let dump = rl.dump().write_to_string()?;
        println!("After purge:\n{}", dump);
        assert_eq!(
            indoc! {r#"
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
            "#},
            dump
        );
    }

    Ok(())
}

/// A purge record that became durable while its chunk removal never ran must
/// not leave the covered chunk files behind: reopening re-derives them from the
/// replayed `purged` state, and the next sync flush deletes them.
#[test]
fn test_reopen_removes_chunks_left_by_an_interrupted_purge()
-> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.wal.chunk_max_records = Some(2);

    let dir = ctx.config.wal.dir.clone();

    {
        let mut rl = ctx.new_raft_log()?;

        for i in 0..8u64 {
            rl.append([((1, i), ss(format!("payload-{}", i)))])?;
        }

        rl.purge((1, 5))?;

        // Make the purge record durable without letting the FlushWorker run
        // the chunk removal, which is the state a crash leaves behind.
        rl.flush(false, None)?;
        rl.wait_worker_idle()?;

        assert_eq!(10, wal_file_count(&dir)?);
    }

    {
        let mut rl = ctx.new_raft_log()?;

        assert_eq!(Some((1, 5)), rl.log_state().purged);
        assert_eq!(10, wal_file_count(&dir)?);

        blocking_flush(&mut rl)?;
        rl.wait_worker_idle()?;

        assert_eq!(4, wal_file_count(&dir)?);

        let got = rl.read(6, 10).collect::<Result<Vec<_>, io::Error>>()?;
        assert_eq!(
            vec![((1, 6), ss("payload-6")), ((1, 7), ss("payload-7"))],
            got
        );
    }

    Ok(())
}

/// Records that carry no log entry still close chunks. Once the log is fully
/// purged, such a chunk is obsolete the moment it closes, and the next sync
/// flush must delete it without waiting for another `purge` call.
#[test]
fn test_chunks_closed_below_purge_point_are_removed() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.wal.chunk_max_records = Some(2);

    let dir = ctx.config.wal.dir.clone();
    let mut rl = ctx.new_raft_log()?;

    rl.append([((1, 0), ss("p0")), ((1, 1), ss("p1"))])?;
    rl.purge((1, 1))?;
    blocking_flush(&mut rl)?;
    rl.wait_worker_idle()?;

    assert_eq!(1, wal_file_count(&dir)?);

    // No further purge: `save_vote` alone closes ten obsolete chunks.
    for term in 2..12u64 {
        rl.save_vote((term, term))?;
    }
    blocking_flush(&mut rl)?;
    rl.wait_worker_idle()?;

    assert_eq!(1, wal_file_count(&dir)?);
    assert_eq!(Some((1, 1)), rl.log_state().purged);

    Ok(())
}

fn wal_file_count(dir: &str) -> Result<usize, io::Error> {
    let count = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().extension().is_some_and(|ext| ext == "wal")
        })
        .count();
    Ok(count)
}

/// Purged items should not reside in cache even when the cache size/item
/// capacity is not reached.
#[test]
fn test_purge_free_cache() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.wal.chunk_max_records = Some(5);
    config.log_cache_max_items = Some(100);
    config.log_cache_capacity = Some(10240);

    {
        let mut rl = ctx.new_raft_log()?;

        build_sample_data(&mut rl)?;

        let stat = rl.stat();
        assert_eq!(stat.payload_cache_item_count, 8);

        rl.purge((2, 6))?;
        blocking_flush(&mut rl)?;

        let stat = rl.stat();
        assert_eq!(stat.payload_cache_item_count, 1);
    }

    Ok(())
}

#[test]
fn test_read_with_cache() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.wal.chunk_max_records = Some(5);
    config.log_cache_max_items = Some(3);

    {
        let mut rl = ctx.new_raft_log()?;
        sample_data::build_sample_data_purge_upto_3(&mut rl)?;

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

        blocking_flush(&mut rl)?;
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

    config.wal.chunk_max_records = Some(5);
    config.log_cache_capacity = Some(0);

    {
        let mut rl = ctx.new_raft_log()?;
        sample_data::build_sample_data_purge_upto_3(&mut rl)?;

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

        blocking_flush(&mut rl)?;
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
            "AccessStat{cache(hit/miss)=5/6}",
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

    config.wal.chunk_max_records = Some(5);
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

    config.wal.chunk_max_records = Some(5);

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

        let flush_stat: Vec<(u64, u64)> =
            rl.wal.get_stat()?.iter().map(FlushStat::offset_sync_id).collect();
        assert_eq!(
            vec![(324, 509), (509, 0)],
            flush_stat,
            "chunk rotations synced old files; current chunk not yet synced"
        );

        blocking_flush(&mut rl)?;

        let flush_stat: Vec<(u64, u64)> =
            rl.wal.get_stat()?.iter().map(FlushStat::offset_sync_id).collect();
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

        let flush_stat: Vec<(u64, u64)> =
            rl.wal.get_stat()?.iter().map(FlushStat::offset_sync_id).collect();
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

    config.wal.chunk_max_records = Some(5);
    config.log_cache_capacity = Some(0);

    {
        let mut rl = ctx.new_raft_log()?;
        sample_data::build_sample_data_purge_upto_3(&mut rl)?;
        assert_eq!(rl.on_disk_size(), 314);
    }
    Ok(())
}

/// `update_state` may change `vote`, `committed` and `user_data` freely, but it
/// must keep `last` and `purged` describing the entries the log holds.
#[test]
fn test_update_state() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.wal.chunk_max_records = Some(5);
    config.log_cache_capacity = Some(0);

    {
        let mut rl = ctx.new_raft_log()?;
        sample_data::build_sample_data_purge_upto_3(&mut rl)?;

        // The log holds `(2,4)..=(2,7)`. This state moves `last` down to
        // index 4 and forgets the purge, so it describes a different log.
        let rejected = rl.update_state(RaftLogState {
            vote: Some((1, 2)),
            last: Some((3, 4)),
            committed: None,
            purged: None,
            user_data: None,
        });
        let err = rejected.unwrap_err();
        let want = "Checkpoint does not match the stored log: the log holds (2, 4)..=(2, 7), the checkpoint declares purged None and last Some((3, 4))";
        assert_eq!(want, err.to_string());

        assert_eq!(rl.log_state(), &RaftLogState {
            vote: None,
            last: Some((2, 7)),
            committed: Some((1, 2)),
            purged: Some((2, 3)),
            user_data: None,
        });

        rl.update_state(RaftLogState {
            vote: Some((1, 2)),
            last: Some((2, 7)),
            committed: None,
            purged: Some((2, 3)),
            user_data: Some(ss("restored")),
        })?;

        blocking_flush(&mut rl)?;
    }

    {
        let rl = ctx.new_raft_log()?;
        assert_eq!(rl.log_state(), &RaftLogState {
            vote: Some((1, 2)),
            last: Some((2, 7)),
            committed: None,
            purged: Some((2, 3)),
            user_data: Some(ss("restored")),
        });

        let got = rl.read(0, 10).collect::<Result<Vec<_>, io::Error>>()?;
        let want = vec![
            ((2, 4), ss("world")),
            ((2, 5), ss("foo")),
            ((2, 6), ss("bar")),
            ((2, 7), ss("wow")),
        ];
        assert_eq!(want, got);
    }
    Ok(())
}

/// A store whose log is empty accepts any state, which is how a caller
/// restores from a snapshot that starts above index 0.
#[test]
fn test_update_state_on_empty_log_accepts_any_state() -> Result<(), io::Error> {
    let (_ctx, mut rl) = new_testing()?;

    rl.update_state(RaftLogState {
        vote: Some((3, 1)),
        last: Some((3, 9)),
        committed: Some((3, 9)),
        purged: Some((3, 9)),
        user_data: Some(ss("from-snapshot")),
    })?;

    rl.append([((3, 10), ss("after"))])?;

    let got = rl.read(0, 20).collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(vec![((3, 10), ss("after"))], got);

    Ok(())
}

#[test]
fn test_open_new_chunk_size() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.wal.chunk_max_records = Some(10000);
    config.wal.chunk_max_size = Some(150);

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
        blocking_flush(&mut rl)?;

        let logs = [
            //
            ((2, 4), ss("world")),
            ((2, 5), ss("foo")),
            ((2, 6), ss("bar")),
            ((2, 7), ss("wow")),
        ];
        rl.append(logs)?;

        blocking_flush(&mut rl)?;

        let want_dumped = indoc! {r#"
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
        ChunkId(00_000_000_000_000_000_474)
          R-00000: [000_000_000, 000_000_066) Size(66): State(RaftLogState { vote: None, last: Some((2, 5)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None })
          R-00001: [000_000_066, 000_000_101) Size(35): Append((2, 6), "bar")
          R-00002: [000_000_101, 000_000_136) Size(35): Append((2, 7), "wow")
        "#};

        let dump = rl.dump().write_to_string()?;
        println!("actual:\n{}", dump);
        assert_eq!(want_dumped, dump);
    }

    Ok(())
}

#[test]
fn test_flush_worker_tracks_new_chunk_file_after_rotation()
-> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    ctx.config.wal.chunk_max_records = Some(5);

    let mut rl = ctx.new_raft_log()?;

    // Append 4 logs. With the initial State record, chunk has 5 records ->
    // full. Next append triggers rotation.
    let logs = [
        ((1, 0), ss("a")),
        ((1, 1), ss("b")),
        ((1, 2), ss("c")),
        ((1, 3), ss("d")),
    ];
    rl.append(logs)?;

    let stat = rl.wal.get_stat()?;
    assert_eq!(2, stat.len(), "should have 2 tracked files after rotation");

    assert_ne!(
        stat[0].ino, stat[1].ino,
        "FlushWorker should track two different files, got same inode {}",
        stat[0].ino
    );

    Ok(())
}

#[test]
fn test_flush_without_sync() -> Result<(), io::Error> {
    // flush(false, _) should hand pending data to the worker (so it reaches
    // the OS file) but must not trigger an fsync; sync_id stays put.
    let ctx = TestContext::new()?;
    let mut rl = ctx.new_raft_log()?;

    rl.append([((1, 0), ss("hello")), ((1, 1), ss("world"))])?;

    let (tx, rx) = sync_channel::<Result<(), io::Error>>(1);
    rl.flush(false, Some(tx))?;
    rx.recv().map_err(|e| io::Error::other(format!("callback: {e}")))??;

    let before: Vec<(u64, u64)> =
        rl.wal.get_stat()?.iter().map(FlushStat::offset_sync_id).collect();
    assert!(
        before.iter().all(|(_, sync_id)| *sync_id == 0),
        "sync_id must stay 0 after flush(false, _), got {:?}",
        before
    );

    // Now a sync flush — sync_id should advance to the write head.
    blocking_flush(&mut rl)?;

    let after: Vec<(u64, u64)> =
        rl.wal.get_stat()?.iter().map(FlushStat::offset_sync_id).collect();
    assert!(
        after.iter().any(|(_, sync_id)| *sync_id > 0),
        "sync_id must advance after flush(true, _), got {:?}",
        after
    );

    Ok(())
}

#[test]
fn test_flush_without_sync_data_visible_after_reopen() -> Result<(), io::Error>
{
    // Data submitted via flush(false, _) reaches the OS file (kernel page
    // cache), so a fresh RaftLog opened on the same chunk path should see
    // the appended entries even though no fsync was ever issued.
    let ctx = TestContext::new()?;

    {
        let mut rl = ctx.new_raft_log()?;
        rl.append([((1, 0), ss("alpha")), ((1, 1), ss("beta"))])?;

        let (tx, rx) = sync_channel::<Result<(), io::Error>>(1);
        rl.flush(false, Some(tx))?;
        rx.recv().map_err(|e| io::Error::other(format!("callback: {e}")))??;
        // rl drops here without ever calling flush(true, _) — only the
        // no-sync path has touched disk.
    }

    let rl = ctx.new_raft_log()?;
    let entries = rl.read(0, 1000).collect::<Result<Vec<_>, io::Error>>()?;
    let payloads: Vec<_> =
        entries.iter().map(|(_, payload)| payload.clone()).collect();
    assert_eq!(
        vec![ss("alpha"), ss("beta")],
        payloads,
        "appended entries should be readable after reopen"
    );

    Ok(())
}

#[test]
fn test_flush_batch_wait_groups_adjacent_sync_flushes() -> Result<(), io::Error>
{
    let mut ctx = TestContext::new()?;
    ctx.config.wal.flush_batch_wait = Some(Duration::from_millis(500));

    let mut rl = ctx.new_raft_log()?;

    rl.append([((1, 0), ss("alpha"))])?;
    let (tx1, rx1) = sync_channel::<Result<(), io::Error>>(1);
    rl.flush(true, Some(tx1))?;

    rl.append([((1, 1), ss("beta"))])?;
    let (tx2, rx2) = sync_channel::<Result<(), io::Error>>(1);
    rl.flush(true, Some(tx2))?;

    rx1.recv().map_err(|e| io::Error::other(format!("callback: {e}")))??;
    rx2.recv().map_err(|e| io::Error::other(format!("callback: {e}")))??;

    let metrics = rl.stat().flush_metrics;
    assert!(
        metrics.batch_size_max >= 2,
        "expected adjacent sync flushes to share a batch, got {metrics:?}"
    );
    assert_eq!(1, metrics.sync_batch_count, "got {metrics:?}");
    assert_eq!(2, metrics.write_request_count, "got {metrics:?}");

    Ok(())
}

#[test]
fn test_flush_batch_max_items_limits_grouping() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    ctx.config.wal.flush_batch_wait = Some(Duration::from_millis(500));
    ctx.config.wal.flush_batch_max_items = Some(1);

    let mut rl = ctx.new_raft_log()?;

    rl.append([((1, 0), ss("alpha"))])?;
    let (tx1, rx1) = sync_channel::<Result<(), io::Error>>(1);
    rl.flush(true, Some(tx1))?;

    rl.append([((1, 1), ss("beta"))])?;
    let (tx2, rx2) = sync_channel::<Result<(), io::Error>>(1);
    rl.flush(true, Some(tx2))?;

    rx1.recv().map_err(|e| io::Error::other(format!("callback: {e}")))??;
    rx2.recv().map_err(|e| io::Error::other(format!("callback: {e}")))??;

    let metrics = rl.stat().flush_metrics;
    assert_eq!(1, metrics.batch_size_max, "got {metrics:?}");
    assert_eq!(2, metrics.sync_batch_count, "got {metrics:?}");
    assert_eq!(2, metrics.write_request_count, "got {metrics:?}");

    Ok(())
}

#[test]
fn test_stat() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    config.wal.chunk_max_records = Some(5);

    {
        let mut rl = ctx.new_raft_log()?;
        sample_data::build_sample_data(&mut rl)?;

        let stat = rl.stat();
        let want = indoc! {r#"
        Stat{
         closed_chunks: [
          ChunkStat(ChunkId(00_000_000_000_000_000_000)){records: 5, [000_000_000, 000_000_161), size: 000_000_161, log_state: RaftLogState { vote: None, last: Some((1, 3)), committed: None, purged: None, user_data: None }},
          ChunkStat(ChunkId(00_000_000_000_000_000_161)){records: 5, [000_000_161, 000_000_324), size: 000_000_163, log_state: RaftLogState { vote: None, last: Some((2, 3)), committed: Some((1, 2)), purged: None, user_data: None }},
          ChunkStat(ChunkId(00_000_000_000_000_000_324)){records: 5, [000_000_324, 000_000_509), size: 000_000_185, log_state: RaftLogState { vote: None, last: Some((2, 6)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None }}
         ],
         open_chunk: ChunkStat(ChunkId(00_000_000_000_000_000_509)){records: 2, [000_000_509, 000_000_610), size: 000_000_101, log_state: RaftLogState { vote: None, last: Some((2, 7)), committed: Some((1, 2)), purged: Some((1, 1)), user_data: None }},
         payload_cache:{evictable: ..=(2, 6),item/max: 000_000_008 / 000_100_000,size/cap: 000_000_029 / 1_073_741_824,miss: 000_000_000,hit: 000_000_000}
        }"#};
        assert_eq!(want, format!("{:#}", stat));
    }

    Ok(())
}
