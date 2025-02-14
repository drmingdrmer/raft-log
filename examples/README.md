# raft-log Examples

This directory contains examples demonstrating how to use the `raft-log` crate, which provides a high-performance, reliable local disk based log storage implementation for the raft consensus protocol.

## Basic Usage Example

The [`basic_usage.rs`](basic_usage.rs) example demonstrates the core functionality of `raft-log`, showing how to:

- Create and open a raft log store
- Save vote(election/leadership) information during elections
- Append log entries (as a leader or follower)
- Update commit index
- Read committed entries
- Get current log state
- Use asynchronous flush with callbacks

### Key Features Demonstrated

1. **Type Safety**: The example shows how to define application-specific types through the `Types` trait:
   - `LogId`: Represents the identity of a log entry (term, index)
   - `LogPayload`: The actual data stored in log entries
   - `Vote`: Vote information for elections
   - `UserData`: Custom user data
   - `Callback`: For async operation notifications

2. **Async Operations**: All write operations are asynchronous for better performance:
   - Changes are batched in memory
   - Explicit `flush()` call with callback when data needs to be persisted
   - Callback notification when flush completes

3. **Core raft Operations**:
   - Vote persistence for election safety
   - Log entry append for replication
   - Commit index updates
   - Log entry reads for state machine application

### Running the Example

```bash
cargo run --example basic_usage
```

This will create a temporary directory and demonstrate the basic operations of the raft log store.