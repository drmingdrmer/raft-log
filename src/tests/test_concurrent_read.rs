use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::thread;

use crate::api::raft_log_writer::RaftLogWriter;
use crate::api::raft_log_writer::blocking_flush;
use crate::tests::context::TestContext;

/// Test concurrent reads from the same RaftLog.
///
/// This test reproduces the race condition in `Chunk::read_record` where
/// multiple threads share the same `Arc<File>` and the non-atomic `seek + read`
/// operation causes threads to read from wrong file positions.
///
/// The race condition sequence:
/// ```text
/// Thread A: seek(position_A)
/// Thread B: seek(position_B)      ← Overwrites Thread A's position
/// Thread A: read()                ← Reads from position_B instead of position_A
/// ```
///
/// Symptoms when the bug is present:
/// - Error: `Unknown record type: XXXXXX` (reading payload data as record type)
/// - Assertion failure: log_id mismatch in load_log_payload
/// - Error: `failed to fill whole buffer` (reading past EOF due to wrong seek)
#[test]
fn test_concurrent_read_race_condition() -> Result<(), io::Error> {
    let mut ctx = TestContext::new()?;
    let config = &mut ctx.config;

    // Small chunk to create multiple closed chunks quickly
    config.chunk_max_records = Some(5);
    // Disable cache to force all reads to go through disk (seek + read)
    config.log_cache_capacity = Some(0);

    let raft_log = {
        let mut rl = ctx.new_raft_log()?;

        // Create many log entries across multiple chunks.
        // With chunk_max_records=5, this creates multiple closed chunks.
        let num_entries = 50;
        for i in 0..num_entries {
            let payload = format!("payload_{:04}", i);
            rl.append([((1, i), payload)])?;
        }

        blocking_flush(&mut rl)?;

        // Drop the mutable raft_log and reopen as shared
        drop(rl);
        ctx.new_raft_log()?
    };

    // Share the RaftLog across threads using Arc
    // Note: RaftLog::read() takes &self, so we can share it across threads
    let raft_log = Arc::new(raft_log);

    let num_threads = 8;
    let iterations_per_thread = 100;
    let num_entries = 50u64;

    let error_count = Arc::new(AtomicUsize::new(0));
    let mismatch_count = Arc::new(AtomicUsize::new(0));
    let panic_count = Arc::new(AtomicUsize::new(0));

    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let rl = raft_log.clone();
        let errors = error_count.clone();
        let mismatches = mismatch_count.clone();
        let panics = panic_count.clone();

        let handle = thread::spawn(move || {
            for iter in 0..iterations_per_thread {
                // Each thread reads different indices to maximize contention
                // on the shared file handle
                let start_idx =
                    ((thread_id + iter) % num_entries as usize) as u64;
                let end_idx = (start_idx + 5).min(num_entries);

                // Use catch_unwind to handle panics from debug_assert in
                // load_log_payload
                let result = std::panic::catch_unwind(
                    std::panic::AssertUnwindSafe(|| {
                        for result in rl.read(start_idx, end_idx) {
                            match result {
                                Ok((log_id, payload)) => {
                                    let expected_index = log_id.1;
                                    let expected_payload = format!(
                                        "payload_{:04}",
                                        expected_index
                                    );

                                    if payload != expected_payload {
                                        mismatches
                                            .fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                                Err(_e) => {
                                    errors.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                    }),
                );

                if result.is_err() {
                    // Panic occurred (likely debug_assert failure in
                    // load_log_payload)
                    panics.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        let _ = handle.join();
    }

    let total_errors = error_count.load(Ordering::Relaxed);
    let total_mismatches = mismatch_count.load(Ordering::Relaxed);
    let total_panics = panic_count.load(Ordering::Relaxed);

    println!(
        "Concurrent read test completed: {} errors, {} mismatches, {} panics",
        total_errors, total_mismatches, total_panics
    );

    // The test expects errors, mismatches, or panics when the race condition is
    // present. Once the bug is fixed (using pread), this assertion should
    // pass.
    assert_eq!(
        total_errors + total_mismatches + total_panics,
        0,
        "Race condition detected: {} errors, {} data mismatches, {} panics",
        total_errors,
        total_mismatches,
        total_panics
    );

    Ok(())
}
