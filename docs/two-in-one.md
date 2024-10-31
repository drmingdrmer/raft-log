# Architecture Design for the "raft-log" Repository

The "raft-log" repository is a Rust implementation of a write-ahead log (WAL) designed for use in Raft, a consensus algorithm for distributed systems. This architecture emphasizes modularity, data integrity, and performance optimization through efficient storage management and caching.

---

## Table of Contents

1. [Overview](#overview)
2. [Key Components](#key-components)
   - [1. Configuration Management](#1-configuration-management)
   - [2. Chunk Management](#2-chunk-management)
   - [3. State Management](#3-state-management)
   - [4. Record Handling](#4-record-handling)
   - [5. Cache Management](#5-cache-management)
   - [6. Raft Log Core](#6-raft-log-core)
   - [7. Concurrency Control](#7-concurrency-control)
   - [8. Utilities and Helpers](#8-utilities-and-helpers)
   - [9. Testing Infrastructure](#9-testing-infrastructure)
   - [10. Command-Line Tool](#10-command-line-tool)
3. [Data Flow and Interactions](#data-flow-and-interactions)
   - [1. Appending Log Entries](#1-appending-log-entries)
   - [2. Reading Log Entries](#2-reading-log-entries)
   - [3. State Updates](#3-state-updates)
   - [4. Synchronization](#4-synchronization)
4. [Error Handling and Data Integrity](#error-handling-and-data-integrity)
5. [Extensibility and Customization](#extensibility-and-customization)
6. [Conclusion](#conclusion)
7. [Diagram: In-Memory and On-Disk Data Layout](#diagram-in-memory-and-on-disk-data-layout)
   - [Explanation](#explanation)
     - [In-Memory Data Structures](#in-memory-data-structures)
     - [On-Disk Data Structures](#on-disk-data-structures)
     - [Data Flow and Interactions](#data-flow-and-interactions-1)
     - [Interactions Between Components](#interactions-between-components)

---

## Overview

The "raft-log" system is structured into several key modules and components, each responsible for different aspects of the log management process. The overall design promotes separation of concerns, allowing for easier maintenance and potential enhancements.

---

## Key Components

### 1. Configuration Management

**Module:** `config.rs`

#### Responsibilities

- Define and manage configuration parameters for the Raft log system.
- Provide utilities for constructing file paths and parsing chunk file names.

#### Key Features

- **Config Struct:**
  - Holds configuration options such as directory paths, cache sizes, buffer sizes, chunk parameters, and data integrity options.
- **File Naming Conventions:**
  - Generates and parses chunk file names using a consistent and sortable format based on chunk IDs.

---

### 2. Chunk Management

**Module:** `chunk.rs`

#### Responsibilities

- Handle storage and retrieval of log records within chunk files on disk.
- Manage active and sealed chunks to optimize write and read operations.

#### Key Components

- **Chunk Struct:**
  - Represents a chunk file containing a sequence of log records.
  - Manages file I/O operations, offsets, and synchronization.
- **ActiveChunk:**
  - The current chunk where new log records are appended.
  - Monitors size and record count to determine when to seal.
- **SealedChunk:**
  - A read-only chunk that has been sealed (no further writes).
  - Retains the state of the log at the time of sealing for recovery purposes.

#### Functionality

- **Appending Records:**
  - Writes new records to the active chunk, updating offsets and ensuring data is flushed to disk.
- **Sealing Chunks:**
  - When a chunk reaches its size or record count limit, it is sealed, and a new active chunk is created.
- **Reading Records:**
  - Provides methods to read records by offset, handling both active and sealed chunks transparently.

---

### 3. State Management

**Modules:** `state.rs`, `raft_log_state.rs`

#### Responsibilities

- Maintain the current state of the Raft log, including voting information and indices of the last, committed, and purged log entries.
- Apply state transitions based on log record operations.

#### Key Components

- **RaftLogState Struct:**
  - Tracks important log indices (`last`, `committed`, `purged`) and the current vote.
  - Provides methods to update state while enforcing consistency rules.
- **State Machine Trait (`state_machine.rs`):**
  - Defines an interface for applying log records to the state.
  - Implemented by `RaftLogSM` to process records and update the `RaftLogState`.

#### Data Integrity

- Enforces rules to prevent inconsistencies:
  - **Vote Reversal:** Rejects attempts to update the vote to an earlier term.
  - **Log ID Reversal:** Prevents appending log IDs that are less than the current `last` log ID.
  - **Non-Consecutive Log IDs:** Ensures log IDs are contiguous unless explicitly truncated.

---

### 4. Record Handling

**Module:** `record.rs`

#### Responsibilities

- Define the structure of different types of log records.
- Serialize and deserialize records for storage and transmission.

#### Record Types

- **SaveVote:** Records a vote with term and candidate information.
- **Append:** Adds a new log entry with a log ID and payload.
- **Commit:** Marks a log entry as committed.
- **TruncateAfter:** Truncates the log after a specified log ID.
- **PurgeUpto:** Removes log entries up to a specified log ID.
- **State:** Captures the entire `RaftLogState` for recovery.

#### Serialization

- Implements the `Encode` and `Decode` traits.
- Utilizes checksums to ensure data integrity during serialization and deserialization.

---

### 5. Cache Management

**Module:** `log_cache.rs`

#### Responsibilities

- Provide an in-memory cache for recent log entries to optimize read performance.

#### Features

- **LogCache Struct:**
  - Stores log entries in a `BTreeMap` for ordered access.
  - Tracks total size and item count to enforce cache limits.
- **Eviction Policy:**
  - Removes the oldest entries when exceeding `max_items` or `capacity`.
  - Ensures the cache remains within configured resource constraints.

---

### 6. Raft Log Core

**Module:** `raft_log.rs`

#### Responsibilities

- Serve as the central orchestrator of the Raft log system.
- Manage interactions between the state machine, WAL, chunk management, and caching.

#### Implementations

- **RaftLog Struct:**
  - Implements the `RaftLogWriter` trait, providing methods like `save_vote`, `append`, `truncate`, `purge`, `commit`, and `sync`.
  - Maintains instances of `RaftLogWAL` and `RaftLogSM` for WAL operations and state management.

#### Data Flow

- **Appending Entries:**
  - Writes to the active chunk and updates the state machine.
- **Reading Entries:**
  - Retrieves from cache if available; otherwise, reads from the appropriate chunk.
- **Synchronization:**
  - Ensures all writes are flushed to disk using the `sync` method.

#### Error Handling

- Propagates errors from underlying components.
- Provides detailed error messages with context to aid in debugging.

---

### 7. Concurrency Control

**Module:** `lock_file.rs`

#### Responsibilities

- Prevent concurrent access to the log directory by multiple processes.
- Ensure data integrity by enforcing exclusive file locks.

#### Mechanism

- **LockFile Struct:**
  - Creates and maintains a lock file using file system locks.
  - Releases the lock when the `LockFile` instance is dropped.

#### Use Cases

- **Process Isolation:** Ensures that only one instance of the application is interacting with the log at any time.
- **Error Reporting:** Provides informative errors if a lock cannot be acquired.

---

### 8. Utilities and Helpers

#### OffsetReader (`offset_reader.rs`)

- Wraps a reader to keep track of the current offset during reading operations.
- Aids in error reporting by providing precise locations of read failures.

#### Number Formatting (`num.rs`)

- Offers utilities for formatting numbers with fixed widths and separators.
- Used for generating chunk file names with sortable and readable numeric components.

#### Error Definitions (`errors.rs`)

- Defines custom error types:
  - Invalid chunk file names.
  - Vote and log ID inconsistencies.
  - Log index not found.

---

### 9. Testing Infrastructure

**Directory:** `tests/`

#### Responsibilities

- Provide comprehensive tests to ensure the correctness and reliability of the system.

#### Components

- **Test Modules (`mod.rs`, `test_raft_log.rs`, `test_context.rs`):**
  - Contain unit tests for individual components and integration tests for system-wide behaviors.
- **Test Utilities (`testing.rs`):**
  - Offer mock implementations and helper functions to simulate various scenarios.

---

### 10. Command-Line Tool

**File:** `src/bin/dump.rs`

#### Responsibilities

- Offer a utility for inspecting and dumping the contents of the Raft log.
- Aid in debugging and analysis of log data.

#### Features

- **Command-Line Interface:**
  - Accepts a path to the log directory.
  - Provides options to format and output log contents.
- **Dump Functionality:**
  - Reads log records from chunks and outputs them in a human-readable format.

---

## Data Flow and Interactions

### 1. Appending Log Entries

**Process:**

1. **API Invocation:**
   - Users invoke `append`, providing log entries.
2. **Record Creation:**
   - Entries are serialized into `Record::Append` records.
3. **Writing to ActiveChunk:**
   - Records are written to the `ActiveChunk`.
4. **State Update:**
   - The `RaftLogState` is updated via the `StateMachine`.
5. **Chunk Management:**
   - If the active chunk reaches capacity, it is sealed.

**Components Involved:**

- `raft_log.rs`
- `chunk.rs`
- `state.rs`

---

### 2. Reading Log Entries

**Process:**

1. **Cache Check:**
   - Users invoke `read` with a range of indices.
   - The system checks the `LogCache` for entries.
2. **Disk Access (if necessary):**
   - Missing entries are loaded from chunk files on disk.
3. **Return Entries:**
   - Entries are returned to the caller.

**Components Involved:**

- `raft_log.rs`
- `log_cache.rs`
- `chunk.rs`

---

### 3. State Updates

**Process:**

1. **Operation Invocation:**
   - Operations like `commit`, `truncate`, and `purge` are invoked.
2. **Record Generation:**
   - Corresponding records are created and written to the log.
3. **State Application:**
   - Records are applied to the `RaftLogState`.
4. **Consistency Enforcement:**
   - The state ensures consistency and integrity rules are enforced.

**Components Involved:**

- `raft_log.rs`
- `state.rs`
- `record.rs`

---

### 4. Synchronization

**Process:**

1. **Sync Invocation:**
   - Users invoke `sync` to flush data to disk.
2. **Disk Synchronization:**
   - All chunks with pending writes are synced using `fsync`.
3. **Durability Assurance:**
   - Ensures durability of log records.

**Components Involved:**

- `raft_log.rs`
- `chunk.rs`

---

## Error Handling and Data Integrity

- **Checksums and Validation:**
  - Records include checksums to detect corruption during read/write operations.
  - Errors are raised if data inconsistencies are detected.

- **Custom Error Types:**
  - Provides granular error definitions to aid in troubleshooting.
  - Includes context in error messages for easier debugging.

- **Recovery Mechanisms:**
  - On startup, the log system can detect incomplete or corrupted records.
  - Implements strategies to handle such scenarios, like truncating incomplete records if configured.

---

## Extensibility and Customization

- **Trait-Based Design:**
  - Key abstractions are defined using traits (`Types`, `StateMachine`, `WAL`), allowing for flexibility.
  - Users can implement these traits to customize behavior (e.g., different log ID or payload types).

- **Configuration Options:**
  - The `Config` struct exposes various parameters to tune performance and behavior.
  - Includes options for cache sizes, chunk sizing, buffer sizes, and data integrity preferences.

---

## Conclusion

The "raft-log" repository presents a robust and flexible architecture for a write-ahead log tailored for Raft consensus operations. By modularizing components and emphasizing data integrity and performance, it serves as a reliable foundation for distributed systems requiring consistent and durable logging mechanisms.

**Key Strengths:**

- **Modularity:** Clear separation of concerns facilitates maintenance and future enhancements.
- **Data Integrity:** Strong emphasis on error detection and state consistency.
- **Performance Optimization:** Efficient disk I/O through chunking and in-memory caching.
- **Customizability:** Configurable parameters and trait-based abstractions allow adaptation to various use cases.
- **Comprehensive Testing:** Extensive tests ensure reliability and correctness.

**Potential Improvements:**

- **Concurrency Enhancements:** Introducing fine-grained locks could improve performance in multi-threaded contexts.
- **Metrics and Monitoring:** Integrating monitoring tools could provide insights into performance characteristics.
- **Documentation:** Expanding documentation, including module-level descriptions and usage examples, would benefit users and contributors.

---

## Diagram: In-Memory and On-Disk Data Layout

Below is a diagram illustrating the in-memory and on-disk data layout of the "raft-log" repository's architecture.

```
+------------------------------------------+
|               Application                |
|                                          |
|  +-----------------------------------+   |
|  |           RaftLog API             |   |
|  | (implements RaftLogWriter trait)  |   |
|  +----------------+------------------+   |
|                   |                      |
|                   v                      |
|  +----------------+------------------+   |
|  |             RaftLog Struct        |   |
|  +----------------+------------------+   |
|                   |                      |
+-------------------|----------------------+
                    |
                    v
+------------------------------------------+
|             In-Memory Data               |
|                                          |
|  +----------------+    +---------------+ |
|  | RaftLogState   |    |   LogCache    | |
|  | - vote         |    | - BTreeMap    | |
|  | - last         |    |   (LogId ->   | |
|  | - committed    |    |    LogPayload)| |
|  | - purged       |    +---------------+ |
|  +----------------+                      |
|          ^                               |
|          |                               |
|  +----------------+                      |
|  |   RaftLogSM    |                      |
|  | (State Machine)|                      |
|  +----------------+                      |
|          |                               |
|          v                               |
|  +----------------+                      |
|  |    LogData     |                      |
|  | - BTreeMap     |                      |
|  |   (Index ->    |                      |
|  |     LogData)   |                      |
|  +----------------+                      |
|                                          |
|  +----------------+   +----------------+ |
|  |  RaftLogWAL    |   |   LockFile     | |
|  | - ActiveChunk  |   +----------------+ |
|  | - SealedChunks |                      |
|  +----------------+                      |
|          |                               |
|          v                               |
|  +----------------+                      |
|  |   ActiveChunk  |                      |
|  | - Chunk        |                      |
|  |   - File       |                      |
|  |   - Offsets    |                      |
|  | - WriteBuffer  |                      |
|  +----------------+                      |
|                                          |
+------------------------------------------+
                    |
                    v
+------------------------------------------+
|              On-Disk Data                |
|                                          |
|  +----------------+   +---------------+  |
|  |   Lock File    |   |  Chunk Files  |  |
|  |  ("LOCK")      |   | (r-*.wal)     |  |
|  +----------------+   |               |  |
|                       | - ActiveChunk |  |
|                       | - SealedChunks|  |
|                       +---------------+  |
|                                          |
+------------------------------------------+
```

### Explanation

#### In-Memory Data Structures

- **RaftLogState (`RaftLogState`):**
  - Maintains the state of the Raft log, including:
    - `vote`: The current term and candidate voted for.
    - `last`: The ID of the last log entry appended.
    - `committed`: The ID of the last committed log entry.
    - `purged`: The ID of the last purged log entry.
  - Ensures that operations maintain the consistency and integrity of the log state.

- **LogCache (`log_cache.rs`):**
  - An in-memory cache for recently accessed log entries.
  - Implemented as a `BTreeMap` of `LogId` to `LogPayload`.
  - Improves read performance by caching frequently accessed entries.
  - Enforces size constraints to prevent excessive memory usage.

- **RaftLogSM (`raft_log_sm.rs`):**
  - The state machine responsible for applying log records to the `RaftLogState`.
  - Processes records like `Append`, `Commit`, `TruncateAfter`, and `PurgeUpto`.

- **LogData (`log_data.rs`):**
  - Stores metadata about log entries.
  - Implemented as a `BTreeMap` mapping log indices to `LogData` objects.

- **RaftLogWAL (`raft_log_wal.rs`):**
  - Manages write-ahead logging functionality.
  - Handles the writing of log records to disk.

- **LockFile (`lock_file.rs`):**
  - Ensures exclusive access to the log directory by the process.

#### On-Disk Data Structures

- **Lock File:**
  - A file named `"LOCK"` located in the log directory.

- **Chunk Files (`chunk.rs`):**
  - Stored in the log directory with filenames like `r-<chunk_id>.wal`.
  - Two types: Active Chunk File and Sealed Chunk Files.

#### Data Flow and Interactions

1. **Appending Log Entries:**
   - The application calls the `append` method on the `RaftLog` API.
   - Records are serialized and written to the `ActiveChunk`.
   - The `RaftLogSM` applies the records to update the `RaftLogState`.
   - If the `ActiveChunk` exceeds limits, it is sealed.

2. **Reading Log Entries:**
   - The `LogCache` is checked first for the requested entries.
   - If not in cache, the `LogData` provides the `chunk_id` and `record_segment`.
   - The appropriate chunk file is accessed to read the record.

#### Interactions Between Components

- **RaftLog API and RaftLog Struct:**
  - Interfaces for application interaction with the log.

- **RaftLogSM (State Machine):**
  - Applies log records and updates `RaftLogState`.

- **RaftLogWAL (Write-Ahead Log):**
  - Manages writing records to disk and chunk management.

- **LogCache and LogData:**
  - Work together to provide efficient read access.

- **LockFile:**
  - Controls access to the log directory.

---

This reorganized architecture design provides a clear and concise understanding of the "raft-log" repository's structure and functionality, facilitating easier comprehension for developers and users interacting with the system.
