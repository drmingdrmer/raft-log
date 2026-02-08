# Architecture

A Rust write-ahead log (WAL) for [Raft](https://en.wikipedia.org/wiki/Raft_(algorithm)) consensus.
Core log types are parameterized by a `Types` trait that defines `LogId`, `LogPayload`, `Vote`, `Callback`, and `UserData`.

## Types Trait

Implementors define this trait first to parameterize all core types:

```rust
pub trait Types
where Self: Debug + Default + PartialEq + Eq + Clone + 'static
{
    type LogId: Debug + Clone + Ord + Eq + Codec + Send + Sync + 'static;
    type LogPayload: Debug + Clone + Codec + Send + Sync + 'static;
    type Vote: Debug + Clone + PartialOrd + Eq + Codec + 'static;
    type Callback: Callback + Send + 'static;
    type UserData: Debug + Clone + Eq + Codec + 'static;

    fn log_index(log_id: &Self::LogId) -> u64;
    fn payload_size(payload: &Self::LogPayload) -> u64;
    fn next_log_index(log_id: Option<&Self::LogId>) -> u64; // has default impl
}
```

`LogId` and `LogPayload` require `Send + Sync` for concurrent cache access.
Serialization uses the `codeq` crate (`Codec` trait).

## Module Structure

```
src/
|-- api/           Public traits: Types, RaftLogWriter, WAL, StateMachine
|-- chunk/         On-disk chunks: Chunk<T>, OpenChunk<T>, ClosedChunk<T>
|-- raft_log/
|   |-- wal/           WAL engine: RaftLogWAL<T>, WALRecord<T>, FlushWorker<T>
|   |-- state_machine/ In-memory state: RaftLogState<T>, PayloadCache<T>
|   +-- log_data.rs    LogData<T>: per-entry metadata (chunk ref + segment)
|-- errors.rs      Error types: VoteReversal, LogIdReversal, LogIdNonConsecutive
|-- types.rs       Segment, Checksum (CRC32)
|-- config.rs      Configuration
+-- file_lock.rs   Exclusive directory lock via fs2
```

## Data Layout

```
+-------------------------------------------------------+
|                       RaftLog<T>                      |
|                 impl RaftLogWriter<T>                 |
|                                                       |
|  +--------------------+   +------------------------+  |
|  | RaftLogWAL<T>      |   | RaftLogStateMachine<T> |  |
|  |                    |   |                        |  |
|  |  OpenChunk<T>      |   |  RaftLogState<T>       |  |
|  |   Arc<File>        |   |   vote, last,          |  |
|  |   global_offsets   |   |   committed, purged,   |  |
|  |   record_write_buf |   |   user_data            |  |
|  |                    |   |                        |  |
|  |  BTreeMap<ChunkId, |   |  BTreeMap<index,       |  |
|  |   ClosedChunk<T>>  |   |   LogData<T>>          |  |
|  |    Arc<File>       |   |  PayloadCache<T>       |  |
|  |    global_offsets  |   |   Arc<RwLock<..>>      |  |
|  |    RaftLogState<T> |   |   size-bounded         |  |
|  +--------------------+   +------------------------+  |
|      |                                                |
|      | FlushRequest (mpsc channel)                    |
|      v                                                |
|  +-------------------+   +----------+                 |
|  | FlushWorker<T>    |   | FileLock |                 |
|  |  bg thread        |   |  "LOCK"  |                 |
|  |  batched fsync    |   +----------+                 |
|  +-------------------+                                |
+-------------------------------------------------------+
                        |
                        v
              On-Disk: <dir>/
                LOCK
                r-00_000_000_000_000_000_000.wal
                ...
```

Both `OpenChunk<T>` and `ClosedChunk<T>` contain a shared inner `Chunk<T>` struct
that holds `Arc<File>` and `global_offsets`.
`ChunkId` is a global byte offset (not a sequence number) -- the filename encodes
this offset with zero-padded digits.

## Threading Model

`RaftLog` writes to the open chunk on the caller's thread.
`FlushWorker` runs on a dedicated background thread (`raft_log_wal_flush_worker`),
receiving `FlushRequest` messages via an `mpsc` channel.
It batches pending file syncs, calls `sync_data()`, then invokes flush callbacks.
This separation allows writes to proceed without blocking on `fsync`.

## WAL Record Types

The WAL supports six record types (`WALRecord<T>`):

- `SaveVote(vote)` -- persist election vote
- `Append(log_id, payload)` -- append a log entry
- `Commit(log_id)` -- mark entry as committed
- `TruncateAfter(log_id)` -- discard entries after log_id (Raft conflict resolution)
- `PurgeUpto(log_id)` -- purge entries up to log_id
- `State(state)` -- full state snapshot (written at chunk creation for recovery)

Each record is serialized with a CRC32 checksum via the `codeq` framework.

## Write Path

1. `RaftLog::append(entries)` encodes each entry as `WALRecord::Append(log_id, payload)`.
2. `RaftLogWAL::append()` (implementing the `WAL` trait) delegates to `OpenChunk::append_record()`, which serializes the record with CRC32 checksum into the chunk file.
3. `RaftLogStateMachine::apply()` updates `RaftLogState`, inserts metadata into the log index `BTreeMap`, and caches the payload.
4. If the open chunk exceeds `chunk_max_records` or `chunk_max_size`, it becomes a `ClosedChunk` and a new `OpenChunk` is created. The closed chunk's file handle is sent to `FlushWorker` via `FlushRequest::AppendFile`.
5. `RaftLog::flush(callback)` sends `FlushRequest::Flush` to `FlushWorker`, which batches pending syncs, calls `sync_data()` on all relevant files, then invokes callbacks.

## Read Path

1. `RaftLog::read(from, to)` returns an iterator over the range `[from, to)` in `BTreeMap<u64, LogData<T>>`.
2. For each entry, checks `PayloadCache` first (guarded by `Arc<RwLock<..>>`).
3. On cache miss, calls `Chunk::read_record(segment)` which uses `pread` (`read_exact_at`) to read atomically without affecting the file position -- safe for concurrent reads.

## Cache Eviction

`PayloadCache` holds payloads whose log id is after `last_evictable` (un-synced data must stay in memory).
After all closed chunk files are synced, `FlushWorker` calls `set_last_evictable(log_id)` with the last log id from the previous closed chunk, then syncs the current open chunk file.
This allows the cache to evict entries up to that point when it exceeds `max_items` or `capacity`.

## Chunk Lifecycle

1. **Create**: `OpenChunk::create()` writes an initial `WALRecord::State` record (state snapshot for recovery).
2. **Append**: Records are appended sequentially; `global_offsets` tracks each record boundary.
3. **Close**: When full, the open chunk becomes a `ClosedChunk` (read-only, holds a `RaftLogState` snapshot).
4. **Purge**: `purge(upto)` schedules fully-purged chunks for removal. The actual deletion happens when the next `flush()` sends `FlushRequest::RemoveChunks` to `FlushWorker`, ensuring the purge record is durable before files are deleted.
5. **Recovery**: On startup, `Chunk::open()` replays records via `RecordIterator`. Incomplete trailing records are truncated if `truncate_incomplete_record` is enabled. Trailing zero bytes (from EXT4 `data=writeback` mode) are detected and truncated.

## Configuration

| Field | Default | Description |
|---|---|---|
| `dir` | (required) | WAL directory path |
| `log_cache_max_items` | 100,000 | Max cached payloads |
| `log_cache_capacity` | 1 GB | Max cache size in bytes |
| `chunk_max_records` | 1,000,000 | Max records per chunk |
| `chunk_max_size` | 1 GB | Max chunk file size |
| `read_buffer_size` | 64 MB | BufReader capacity for chunk loading |
| `truncate_incomplete_record` | true | Truncate incomplete records on recovery |

## Error Handling

- **State consistency**: `RaftLogState` rejects vote reversals, log ID reversals, and non-consecutive appends (see `errors.rs`).
- **Data integrity**: All records include CRC32 checksums. Corruption is detected on read.
- **File locking**: `FileLock` uses `fs2::try_lock_exclusive()` to prevent concurrent process access.
- **Recovery**: See Chunk Lifecycle step 5.
