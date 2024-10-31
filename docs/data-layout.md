# Diagram Illustrating In-Memory and On-Disk Data Layout

Below is a diagram that illustrates the in-memory and on-disk data layout of the "raft-log" repository's architecture.

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

## Explanation

### In-Memory Data Structures

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
  - Ensures that state transitions are valid and that the `RaftLogState` remains consistent.

- **LogData (`log_data.rs`):**
  - Stores metadata about log entries.
  - Implemented as a `BTreeMap` mapping log indices to `LogData` objects.
  - Each `LogData` includes:
    - `log_id`: Unique identifier for the log entry.
    - `chunk_id`: Identifier of the chunk where the log entry resides.
    - `record_segment`: Byte offset and length of the record within the chunk file.

- **RaftLogWAL (`raft_log_wal.rs`):**
  - Manages write-ahead logging functionality.
  - Handles the writing of log records to disk.
  - Manages:
    - **ActiveChunk:**
      - The current chunk file being written to.
      - Contains a `Chunk` object with file handles and offset tracking.
      - Holds a write buffer for accumulating records before flushing to disk.
    - **SealedChunks:**
      - Completed chunks that are read-only.
      - No further records are appended to these chunks.
      - Retained for reading and replaying log entries.

- **LockFile (`lock_file.rs`):**
  - Ensures exclusive access to the log directory by the process.
  - Prevents concurrent processes from modifying the log simultaneously.
  - Acquired when the `RaftLog` is instantiated and released upon its drop.

### On-Disk Data Structures

- **Lock File:**
  - A file named `"LOCK"` located in the log directory.
  - Used to enforce exclusive access to the log by the process.

- **Chunk Files (`chunk.rs`):**
  - Stored in the log directory with filenames like `r-<chunk_id>.wal`.
    - `<chunk_id>` is a formatted numerical identifier.
  - Two types of chunk files:
    - **Active Chunk File:**
      - The file currently being written to.
      - Corresponds to the `ActiveChunk` in memory.
    - **Sealed Chunk Files:**
      - Read-only files that contain previously written log records.
      - Correspond to the `SealedChunks` in memory.
  - Each chunk file contains:
    - **Offsets:**
      - An array of byte offsets marking the boundaries of records within the file.
    - **Log Records:**
      - Serialized `Record` objects, including:
        - State records (`Record::State`)
        - Append records (`Record::Append`)
        - Commit records (`Record::Commit`)
        - Truncate and purge records

### Data Flow and Interactions

1. **Appending Log Entries:**

   - **Write Path:**
     1. **API Invocation:**
        - The application calls the `append` method on the `RaftLog` API with new log entries.
     2. **Record Creation:**
        - Each log entry is wrapped in a `Record::Append` object.
     3. **ActiveChunk Write:**
        - Records are serialized and written to the `ActiveChunk`'s write buffer.
        - When the buffer reaches a certain size, it is flushed to the active chunk file on disk.
     4. **State Machine Update:**
        - The `RaftLogSM` applies the records to update the `RaftLogState`.
        - The `LogData` map is updated with metadata about the new entries.
     5. **Chunk Management:**
        - If the `ActiveChunk` exceeds configured limits (size or record count), it is sealed.
        - A new `ActiveChunk` is created, and the process repeats.

2. **Reading Log Entries:**

   - **Read Path:**
     1. **Cache Lookup:**
        - The `LogCache` is checked first for the requested entries.
     2. **Cache Miss Handling:**
        - If not in cache, the `LogData` map provides the `chunk_id` and `record_segment` for each entry.
     3. **Disk Access:**
        - The appropriate chunk file (active or sealed) is accessed.
        - The log record is read from the file using the offset and length.
     4. **Cache Update:**
        - The retrieved entry may be added to the `LogCache` for future accesses.

3. **Sealing Chunks:**

   - **When Chunks Are Sealed:**
     - Conditions like reaching a maximum number of records or file size trigger the sealing.
     - The `ActiveChunk`:
       - Flushes any remaining data to disk.
       - Updates its metadata and becomes read-only (`SealedChunk`).
     - A new `ActiveChunk` is initialized for subsequent writes.

4. **State Updates:**

   - **Applying Records:**
     - Operations such as `commit`, `truncate`, or `purge` generate respective records.
     - These records are processed by the `RaftLogSM`, which updates the `RaftLogState`.

5. **Synchronization:**

   - **Data Durability:**
     - The `sync` method ensures that all in-memory buffers are flushed to disk.
     - Calls `fsync` on chunk files to guarantee data persistence.

### Interactions Between Components

- **RaftLog API and RaftLog Struct:**
  - Provides the interface for the application to interact with the log.
  - Delegates operations to the underlying components.

- **RaftLogSM (State Machine):**
  - Validates and applies log records.
  - Ensures the `RaftLogState` remains consistent with the log operations.

- **RaftLogWAL (Write-Ahead Log):**
  - Manages the writing of log records to disk.
  - Handles chunk creation, sealing, and file synchronization.

- **LogCache and LogData:**
  - Work together to provide efficient read access to log entries.
  - The cache reduces disk I/O, while `LogData` maintains the mapping between log indices and records on disk.

- **LockFile:**
  - Operates independently to control access to the log directory.
  - Ensures that the integrity of the log is not compromised by concurrent processes.

---

This diagram and explanation provide a visual representation of how data moves between in-memory structures and on-disk storage in the "raft-log" system. It highlights how components interact to provide efficient, reliable logging for Raft consensus operations, ensuring data integrity and performance through careful management of memory and storage resources.
