# Architecture

A Rust write-ahead log (WAL) for [Raft](https://en.wikipedia.org/wiki/Raft_(algorithm)) consensus.
Core log types are parameterized by a `Types` trait that defines `LogId`, `LogPayload`, `Vote`, `Callback`, and `UserData`.

## The raft-log / chunked-wal split

The chunked log storage lives in a separate crate,
[chunked-wal](https://crates.io/crates/chunked-wal). It owns everything that is
not specific to Raft: chunk files, the record framing and CRC32 checksum, the
background flush worker, the directory lock, and crash recovery.

`chunked-wal` is generic over a `WalTypes` trait with an `Action` type, a
`Checkpoint` type, and a `Callback` type. This crate plugs Raft into those three
slots:

| chunked-wal slot | raft-log fills it with |
|---|---|
| `WalTypes::Action` | `RaftLogAction<T>` |
| `WalTypes::Checkpoint` | `RaftLogState<T>` |
| `WalTypes::Callback` | `T::Callback` |
| `StateMachine<W>` | `RaftLogStateMachine<T>` |

`RaftWalTypes<T>` is the binding type that carries those choices, and
`RaftLogRecord<T>` is an alias for `chunked_wal::WALRecord<RaftWalTypes<T>>`.

So this crate contributes the Raft semantics — which actions exist, what state
they build, which entries a read may serve — while durability, batching and
recovery come from the WAL crate underneath.

## Types Trait

Implementors define this trait first to parameterize all core types:

```rust
pub trait Types
where Self: Debug + Default + PartialEq + Eq + Clone + 'static
{
    type LogId: Debug + Clone + Ord + Eq + Codec + Send + Sync + 'static;
    type LogPayload: Debug + Clone + Codec + Send + Sync + 'static;
    type Vote: Debug + Clone + PartialOrd + Eq + Codec + Send + Sync + 'static;
    type Callback: Callback;
    type UserData: Debug + Clone + Eq + Codec + Send + Sync + 'static;

    fn log_index(log_id: &Self::LogId) -> u64;
    fn payload_size(payload: &Self::LogPayload) -> u64;
    fn next_log_index(log_id: Option<&Self::LogId>) -> u64; // has default impl
}
```

`LogId` and `LogPayload` require `Send + Sync` for concurrent cache access.
`LogId` requires `Ord` because purge and truncate boundaries are compared as
whole log ids, not by index alone.
Serialization uses the `codeq` crate (`Codec` trait).

## Module Structure

```
src/
|-- api/
|   |-- types.rs          Types, RaftWalTypes
|   |-- raft_log_writer.rs  RaftLogWriter<T>: the write API
|   |-- state_machine.rs  Re-export of chunked_wal::StateMachine
|   |-- wal.rs            Re-export of chunked_wal::WAL
|   +-- wal_types.rs      Re-export of chunked_wal::WalTypes
|-- raft_log/
|   |-- raft_log.rs       RaftLog<T>: the entry point
|   |-- raft_log_action.rs  RaftLogAction<T>: the five Raft actions
|   |-- raft_log_record.rs  RaftLogRecord<T> alias
|   |-- state_machine/
|   |   |-- mod.rs        RaftLogStateMachine<T>: log map + cache + state
|   |   |-- raft_log_state.rs  RaftLogState<T>: vote/last/committed/purged
|   |   +-- payload_cache.rs   PayloadCache<T>
|   |-- log_data.rs       LogData<T>: per-entry metadata (chunk ref + segment)
|   |-- dump.rs, dump_api.rs, dump_raft_log.rs   Debug dump of a WAL directory
|   +-- stat.rs, access_state.rs   Cache-hit counters
|-- errors.rs             VoteReversal, LogIdReversal, LogIdNonConsecutive,
|                         LogIdIndexDisorder, CheckpointLogMismatch,
|                         LogIndexNotFound
|-- types.rs              Segment, Checksum (CRC32)
+-- config.rs             Config, wrapping chunked_wal::Config
```

Chunk files, `FlushWorker`, and the directory lock are no longer here; they live
in `chunked-wal`.

## Data Layout

```
+-------------------------------------------------------+
|                       RaftLog<T>                      |
|                 impl RaftLogWriter<T>                 |
|                                                       |
|  +--------------------+   +------------------------+  |
|  | ChunkedWal<        |   | RaftLogStateMachine<T> |  |
|  |   RaftWalTypes<T>> |   |                        |  |
|  |  (chunked-wal)     |   |  RaftLogState<T>       |  |
|  |                    |   |   vote, last,          |  |
|  |  OpenChunk         |   |   committed, purged,   |  |
|  |  BTreeMap<ChunkId, |   |   user_data            |  |
|  |   ClosedChunk>     |   |                        |  |
|  |  FlushClient       |   |  BTreeMap<index,       |  |
|  |  WalLock           |   |   LogData<T>>          |  |
|  +--------------------+   |  PayloadCache<T>       |  |
|      |                    |   Arc<RwLock<..>>      |  |
|      |                    |   size-bounded         |  |
|      |                    +------------------------+  |
|      | WorkerRequest (mpsc channel)                   |
|      v                                                |
|  +-------------------+   +-------------------------+  |
|  | FlushWorker       |   | removed_chunks:         |  |
|  |  bg thread        |   |  Vec<ChunkId>           |  |
|  |  batched fsync    |   |  (purged, not yet       |  |
|  +-------------------+   |   deleted)              |  |
|                          +-------------------------+  |
+-------------------------------------------------------+
                        |
                        v
              On-Disk: <dir>/
                LOCK
                r-00_000_000_000_000_000_000.wal
                ...
```

`ChunkId` is a global byte offset (not a sequence number) -- the filename encodes
this offset with zero-padded digits.

## Threading Model

`RaftLog` writes into the open chunk's in-memory buffer on the caller's thread;
`WAL::append` touches no file. `FlushWorker` runs on a dedicated background
thread (`chunked_wal_flush_worker`), receiving `WorkerRequest` messages over an
`mpsc` channel. It batches pending writes, calls `sync_data()`, then invokes
flush callbacks. This separation lets writes proceed without blocking on
`fsync`.

The worker drains its queue in order, which is what sequences chunk deletion
after the covering purge record's fsync.

## WAL Record Types

A record is either one of five `RaftLogAction<T>` variants or a checkpoint:

- `SaveVote(vote)` -- persist election vote
- `Append(log_id, payload)` -- append a log entry
- `Commit(log_id)` -- mark entry as committed
- `TruncateAfter(log_id)` -- discard entries after log_id (Raft conflict resolution)
- `PurgeUpto(log_id)` -- purge entries up to and including log_id
- `Checkpoint(state)` -- a full `RaftLogState<T>`, written as every chunk's
  first record so that any prefix of chunks can be dropped

The on-disk type tags are `0..=4` for the actions and `5` for the checkpoint.
Each record is serialized with a CRC32 checksum via the `codeq` framework.

## Write Path

1. `RaftLog::append(entries)` encodes each entry as `RaftLogAction::Append(log_id, payload)`.
2. `WAL::append` serializes the record with its checksum into the open chunk's pending buffer. Nothing reaches a file yet.
3. `RaftLogStateMachine::apply()` updates `RaftLogState`, inserts metadata into the log map, and caches the payload.
4. If the open chunk exceeds `chunk_max_records` or `chunk_max_size`, it closes and a new open chunk is created with a leading checkpoint.
5. `RaftLog::flush(sync, callback)` hands the pending records to `FlushWorker`, which writes them, optionally calls `sync_data()`, then invokes the callback.

## Read Path

1. `RaftLog::read(from, to)` returns an iterator over the range `[from, to)` in `BTreeMap<u64, LogData<T>>`.
2. For each entry, checks `PayloadCache` first (guarded by `Arc<RwLock<..>>`).
3. On cache miss, reads the record at its recorded segment using `pread` (`read_exact_at`), which does not move the file position -- safe for concurrent reads.

## Cache Eviction

`PayloadCache` holds payloads whose log id is after `last_evictable`; un-synced
data must stay in memory. After a closed chunk's file is synced, the
`on_chunk_persisted` callback calls `set_last_evictable(log_id)` with the last
log id of the previous chunk's checkpoint. The cache may then evict up to that
point once it exceeds `log_cache_max_items` or `log_cache_capacity`.

## Chunk Lifecycle

1. **Create**: a new chunk's first record is a `Checkpoint(RaftLogState)`, the state snapshot that makes any later chunk self-describing.
2. **Append**: records are appended sequentially; each record's segment records its boundary.
3. **Close**: when full, the open chunk becomes a closed chunk, read-only, holding the state snapshot reached at its end.
4. **Purge**: `RaftLog::purge(upto)` moves every closed chunk whose trailing checkpoint has `last <= purged` into `removed_chunks`. A chunk closing at or below the purge point is buffered the same way. The files are unlinked when the next `flush(true, _)` queues a removal behind the sync, so the purge record is durable before any file disappears.
5. **Recovery**: on startup, `chunked-wal` replays every chunk. A trailing chunk with no complete initial record is deleted, an incomplete trailing record is truncated when `truncate_incomplete_record` is enabled, and a zero-filled tail (from ext4 `data=writeback`) is treated the same way. `RaftLog::open` then rebuilds `removed_chunks` from the replayed `purged` state, so a purge whose removal never ran still takes effect.

## Configuration

`Config` holds the two cache limits and embeds `chunked_wal::Config` as `wal`.

| Field | Default | Description |
|---|---|---|
| `wal.dir` | (required) | WAL directory path |
| `log_cache_max_items` | 100,000 | Max cached payloads |
| `log_cache_capacity` | 1 GB | Max cache size in bytes |
| `wal.chunk_max_records` | 1,000,000 | Max records per chunk |
| `wal.chunk_max_size` | 1 GB | Max chunk file size |
| `wal.read_buffer_size` | 64 MB | BufReader capacity for chunk loading |
| `wal.truncate_incomplete_record` | true | Truncate an incomplete trailing record on recovery |
| `wal.flush_batch_wait` | 1 ms | Time the worker waits to batch more requests |
| `wal.flush_batch_max_items` | 2,048 | Max requests per batch |
| `wal.flush_queue_max_bytes` | 64 MB | Back-pressure limit on queued bytes |

## Error Handling

- **State consistency**: `RaftLogState` rejects vote reversals, log id reversals, and non-consecutive appends. `purge` additionally rejects a log id that conflicts with the log id stored at that index, and `update_state` rejects a state that does not describe the entries the log holds. See `errors.rs`.
- **Data integrity**: every record carries a CRC32 checksum, checked on read.
- **File locking**: `chunked-wal` holds an exclusive directory lock for the lifetime of the WAL, so a second process fails to open with `WouldBlock`.
- **Recovery**: see Chunk Lifecycle step 5.
