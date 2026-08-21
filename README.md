# raft-log

Log Storage for raft

A high-performance, reliable local disk-based log storage implementation for the raft consensus protocol.

## Features

- Type-safe API with generic types for log entries, vote information, and user data
- Asynchronous write operations with callback support
- Efficient batch processing and disk I/O
- Core raft operations support:
  - Vote persistence for election safety
  - Log entry append for replication
  - Commit index management
  - Log entry reads for state machine application

## Example

See [basic usage example](examples/basic_usage.rs) for a complete demonstration of core functionality, including:

- Creating and opening a raft log store
- Saving vote information during elections
- Appending log entries (as a leader or follower)
- Updating commit index
- Reading committed entries
- Getting current log state
- Using asynchronous flush with callbacks

Basic usage:

```rust
use std::io;
use std::sync::Arc;
use std::sync::mpsc::SyncSender;
use std::sync::mpsc::sync_channel;

use raft_log::api::raft_log_writer::RaftLogWriter;
use raft_log::{Config, RaftLog, Types};

// Define your application-specific types
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct MyTypes;

impl Types for MyTypes {
    type LogId = (u64, u64);        // (term, index)
    type LogPayload = String;       // Log entry data
    type Vote = (u64, u64);         // (term, voted_for)
    type UserData = String;         // Custom user data
    type Callback = SyncSender<io::Result<()>>;

    fn log_index(log_id: &Self::LogId) -> u64 {
        log_id.1
    }

    fn payload_size(payload: &Self::LogPayload) -> u64 {
        payload.len() as u64
    }
}

// Open a RaftLog instance
let config = Arc::new(Config::new("/path/to/raft-log-dir"));
let mut raft_log = RaftLog::<MyTypes>::open(config)?;

// Save vote information
raft_log.save_vote((1, 2))?;  // Voted for node-2 in term 1

// Append log entries
let entries = vec![
    ((1, 1), "first entry".to_string()),
    ((1, 2), "second entry".to_string()),
];
raft_log.append(entries)?;

// Update commit index
raft_log.commit((1, 2))?;

// Flush changes to disk with callback
let (tx, rx) = sync_channel(1);
raft_log.flush(true, Some(tx))?;
rx.recv().unwrap()?;
```

## Architecture

[docs/architecture.md](docs/architecture.md) describes the on-disk layout, the
write and read paths, and the split between this crate and
[chunked-wal](https://crates.io/crates/chunked-wal), which owns the chunked
write-ahead log underneath.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
