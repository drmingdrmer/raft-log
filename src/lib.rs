//! Log Storage for raft:
//! A high-performance, reliable local disk-based log storage implementation for
//! the raft consensus protocol.
//!
//! ## Features
//!
//! - Type-safe API with generic types for log entries, vote information, and
//!   user data
//! - Asynchronous write operations with callback support
//! - Efficient batch processing and disk I/O
//! - Core raft operations support:
//!   - Vote persistence for election safety
//!   - Log entry append for replication
//!   - Commit index management
//!   - Log entry reads for state machine application
//!
//! ## Example
//!
//! See [basic usage example](https://github.com/datafuselabs/openraft/blob/main/examples/basic_usage.rs)
//! for a complete demonstration of core functionality, including:
//!
//! Basic usage:
//!
//! ```rust
//! use raft_log::{RaftLog, Config};
//!
//! // Define your application-specific types
//! impl Types for MyTypes {
//!     type LogId = (u64, u64);        // (term, index)
//!     type LogPayload = String;        // Log entry data
//!     type Vote = (u64, u64);         // (term, voted_for)
//!     type UserData = String;         // Custom user data
//!     type Callback = SyncSender<io::Result<()>>;
//! }
//!
//! // Open a RaftLog instance
//! let config = Arc::new(Config::default());
//! let mut raft_log = RaftLog::<MyTypes>::open(config)?;
//!
//! // Save vote information
//! raft_log.save_vote((1, 2))?;  // Voted for node-2 in term 1
//!
//! // Append log entries
//! let entries = vec![
//!     ((1, 1), "first entry".to_string()),
//!     ((1, 2), "second entry".to_string()),
//! ];
//! raft_log.append(entries)?;
//!
//! // Update commit index
//! raft_log.commit((1, 2))?;
//!
//! // Flush changes to disk with callback
//! let (tx, rx) = sync_channel(1);
//! raft_log.flush(tx)?;
//! rx.recv().unwrap()?;
//! ```

mod chunk;
mod config;

pub(crate) mod file_lock;
pub(crate) mod num;
pub(crate) mod offset_reader;
pub(crate) mod raft_log;
pub(crate) mod testing;

pub mod types;
pub use codeq;

pub mod api;
pub mod dump_writer;
pub mod errors;

pub use api::types::Types;
pub use chunk::chunk_id::ChunkId;
pub use config::Config;
pub use raft_log::stat::ChunkStat;
pub use raft_log::stat::Stat;
pub use raft_log::wal::callback::Callback;

pub use self::raft_log::dump::Dump;
pub use self::raft_log::dump_api::DumpApi;
pub use self::raft_log::dump_raft_log::DumpRaftLog;
pub use self::raft_log::dump_raft_log::DumpRaftLogIter;
pub use self::raft_log::raft_log::RaftLog;
pub use self::raft_log::wal::wal_record::WALRecord;
pub use crate::types::Segment;

#[cfg(test)]
mod tests;
