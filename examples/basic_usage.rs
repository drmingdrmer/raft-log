//! This example demonstrates basic usage of RaftLog, including:
//! - Opening a RaftLog
//! - Writing log entries
//! - Reading log entries
//! - Managing vote and commit information

use std::io;
use std::sync::Arc;
use std::sync::mpsc::SyncSender;
use std::sync::mpsc::sync_channel;

use raft_log::Config;
use raft_log::RaftLog;
use raft_log::api::raft_log_writer::RaftLogWriter;
use raft_log::api::types::Types;

// Define our application-specific types used in this RaftLog storage.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct MyTypes;

impl Types for MyTypes {
    // LogId is (term, log_index)
    type LogId = (u64, u64);

    // LogPayload is a simple string
    type LogPayload = String;

    // Vote is `(term, voted_for)`, voted_for is the candidate node id in u64.
    type Vote = (u64, u64);

    // UserData is a simple string
    type UserData = String;

    // Callback is a channel Sender for notifying the caller when the flush is
    // complete.
    type Callback = SyncSender<io::Result<()>>;

    fn log_index(log_id: &Self::LogId) -> u64 {
        log_id.1
    }

    fn payload_size(payload: &Self::LogPayload) -> u64 {
        payload.len() as u64
    }
}

fn main() -> io::Result<()> {
    // Create a temporary directory for RaftLog data
    let temp_dir = tempfile::tempdir()?;
    let config = Arc::new(Config {
        dir: temp_dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    });

    // Open a RaftLog instance
    let mut raft_log = RaftLog::<MyTypes>::open(config)?;

    // Save vote information (e.g., when a candidate starts an election, or a
    // node receives RequestVote request)
    let vote = (1, 2); // Voted for node-2 in term 1
    raft_log.save_vote(vote)?;

    // Append some log entries
    // Each entry has a LogId of (term, log_index) and a payload.
    //
    // For example, when a follower receives a AppendEntries request from a
    // leader.
    let entries = vec![
        ((1, 1), "first entry".to_string()), // term 1, index 1
        ((1, 2), "second entry".to_string()), // term 1, index 2
        ((2, 3), "third entry".to_string()), // term 2, index 3
    ];
    raft_log.append(entries)?;

    // Update commit index (e.g., when entries are replicated to majority)
    // Here we commit up to log index 2.
    //
    // For example, when a leader receives ack from majority of followers for an
    // AppendEntries request, or a follower receives committed index from the
    // leader.
    raft_log.commit((1, 2))?;

    // Receive a callback when the flush is complete.
    // Note that all of the above operations are asynchronous, no data is
    // guaranteed to be persisted until a `flush()` is called.
    {
        // Create a channel for flush callback
        let (tx, rx) = sync_channel(1);

        // Flush changes to disk
        raft_log.flush(Some(tx))?;

        // Wait for flush to complete
        rx.recv().unwrap()?;
    }

    // Read entries in index range `[1, 3)`
    let read_entries: Vec<_> =
        raft_log.read(1, 3).collect::<Result<Vec<_>, _>>()?;

    println!("Read {} entries:", read_entries.len());
    for ((term, idx), payload) in read_entries {
        println!("  Term: {}, Index: {}, Payload: {}", term, idx, payload);
    }

    // Access current state
    let state = raft_log.log_state();
    println!("\nCurrent state:");
    println!("  Last log: {:?}", state.last());
    println!("  Committed: {:?}", state.committed());
    println!("  Vote: {:?}", state.vote());

    Ok(())
}
