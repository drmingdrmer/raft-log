use std::io;
use std::io::Write;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use goldenfile::Mint;
use raft_log::Config;
use raft_log::RaftLog;
use raft_log::Types;
use raft_log::api::raft_log_writer::RaftLogWriter;
use tempfile::TempDir;

// Define the types for our test
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct TestTypes;

impl Types for TestTypes {
    type LogId = (u64, u64); // (term, index)
    type LogPayload = String;
    type Vote = (u64, u64); // (term, node_id)
    type Callback = std::sync::mpsc::SyncSender<Result<(), io::Error>>;
    type UserData = Vec<u8>; // For snapshot data

    fn log_index(log_id: &Self::LogId) -> u64 {
        log_id.1 // Return the index part
    }

    fn payload_size(payload: &Self::LogPayload) -> u64 {
        payload.len() as u64
    }
}

#[test]
fn test_massive_load() -> std::io::Result<()> {
    let mut mint = Mint::new("tests/massive");
    let file = &mut mint.new_goldenfile("periodical-read.txt")?;

    let temp_dir = TempDir::new()?;
    let path = temp_dir.path().to_path_buf().to_str().unwrap().to_string();
    // let path = "tests/foo".to_string();

    let config = Arc::new(Config {
        dir: path,
        log_cache_max_items: Some(200),
        chunk_max_records: Some(100),
        ..Default::default()
    });

    let mut log_index = 0u64;
    let mut purge_index = 0u64;

    // reopen 3 times
    for reopen_index in 0..3 {
        let mut log = RaftLog::<TestTypes>::open(config.clone())?;

        for i in 1u64..500 {
            log_index += 1;
            let log_id = (log_index, log_index);

            // Append new entries
            let entries =
                vec![(log_id, format!("data-{}-{}", reopen_index, log_index))];
            log.append(entries)?;

            // Commit and purge periodically
            if i % 11 == 0 {
                log.commit(log_id)?;
            }

            if i % 13 == 0 {
                log.purge((purge_index, purge_index))?;
                purge_index += 7;
            }

            // Save vote occasionally
            if i % 17 == 0 {
                log.save_vote((log_index, 1u64))?;
            }

            // Truncate the log
            if i % 29 == 0 {
                log.truncate(log_index - 5)?;
                log_index -= 6;
            }

            // Read and verify periodically
            if i % 23 == 0 {
                // deterministic random index between purged and log_index
                let random_index =
                    purge_index + (i * 40503 % (log_index - purge_index + 1));
                let entries = log
                    .read(random_index, random_index + 6)
                    .collect::<Result<Vec<_>, _>>()?;

                let state = log.log_state();
                writeln!(file, "{:?}", state)?;

                let stat = log.stat();
                writeln!(file, "{:#}", stat)?;

                for entry in entries {
                    writeln!(file, "{:?}", entry)?;
                }
            }
        }

        let stat = log.stat();
        writeln!(file, "sync start: {:#}", stat)?;

        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        log.flush(tx)?;
        rx.recv().unwrap()?;

        let stat = log.stat();
        writeln!(file, "sync done: {:#}", stat)?;

        sleep(Duration::from_millis(200));
    }

    Ok(())
}
