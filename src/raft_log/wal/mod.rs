pub(crate) mod callback;
pub(crate) mod flush_request;
pub(crate) mod flush_worker;

use std::collections::BTreeMap;
use std::io;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::sync::RwLock;

use codeq::OffsetSize;
use codeq::Segment;
pub(crate) use flush_request::FlushRequest;
use log::info;

use crate::api::wal::WAL;
use crate::chunk::closed_chunk::ClosedChunk;
use crate::chunk::open_chunk::OpenChunk;
use crate::raft_log::log_data::LogData;
use crate::raft_log::state_machine::payload_cache::PayloadCache;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;
use crate::raft_log::wal::flush_request::Flush;
use crate::raft_log::wal::flush_worker::FileEntry;
use crate::raft_log::wal::flush_worker::FlushWorker;
use crate::ChunkId;
use crate::Config;
use crate::Types;
use crate::WALRecord;

pub(crate) mod wal_record;

#[derive(Debug)]
pub(crate) struct RaftLogWAL<T>
where T: Types
{
    pub(crate) config: Arc<Config>,
    pub(crate) open: OpenChunk<T>,
    pub(crate) closed: BTreeMap<ChunkId, ClosedChunk<T>>,

    flush_tx: SyncSender<FlushRequest<T>>,
}

impl<T> RaftLogWAL<T>
where T: Types
{
    pub(crate) fn new(
        config: Arc<Config>,
        closed: BTreeMap<ChunkId, ClosedChunk<T>>,
        open: OpenChunk<T>,
        cache: Arc<RwLock<PayloadCache<T>>>,
    ) -> Self {
        let last_closed_chunk_state =
            closed.iter().last().map(|(_, c)| c.state.clone());

        let prev_last_log_id =
            last_closed_chunk_state.and_then(|s| s.last().cloned());

        let offset = open.chunk.global_start();
        let f = open.chunk.f.clone();

        let file_entry = FileEntry::new(offset, f, prev_last_log_id);

        let (flush_tx, rx) = std::sync::mpsc::sync_channel(1024);
        let worker = FlushWorker::new(rx, file_entry, cache);

        worker.spawn();

        Self {
            config,
            open,
            closed,
            flush_tx,
        }
    }

    pub(crate) fn send_flush(
        &self,
        callback: T::Callback,
    ) -> Result<(), io::Error> {
        self.flush_tx
            .send(FlushRequest::Flush(Flush {
                upto_offset: self.open.chunk.global_end(),
                callback,
            }))
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to send sync request: {}", e),
                )
            })
    }
    pub(crate) fn send_remove_chunks(
        &self,
        chunk_paths: Vec<String>,
    ) -> Result<(), io::Error> {
        self.flush_tx.send(FlushRequest::RemoveChunks { chunk_paths }).map_err(
            |e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to send remove chunks request: {}", e),
                )
            },
        )
    }

    #[allow(dead_code)]
    pub(crate) fn get_stat(&self) -> Result<Vec<(u64, u64)>, io::Error> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.send_get_stat(tx)?;
        rx.recv().map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to receive get state response: {}", e),
            )
        })
    }

    #[allow(dead_code)]
    pub(crate) fn send_get_stat(
        &self,
        callback: SyncSender<Vec<(u64, u64)>>,
    ) -> Result<(), io::Error> {
        self.flush_tx.send(FlushRequest::GetFlushStat { tx: callback }).map_err(
            |e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to send get state request: {}", e),
                )
            },
        )
    }

    pub(crate) fn is_open_chunk_full(&self) -> bool {
        self.open.chunk.records_count() >= self.config.chunk_max_records()
            || (self.open.chunk.chunk_size() as usize)
                >= self.config.chunk_max_size()
    }

    pub(crate) fn try_close_full_chunk(
        &mut self,
        get_state: impl FnOnce() -> RaftLogState<T>,
    ) -> Result<Option<RaftLogState<T>>, io::Error> {
        if !self.is_open_chunk_full() {
            return Ok(None);
        }

        let config = self.config.clone();
        let offset = self.open.chunk.last_segment().end();

        info!(
            "Closing full chunk: {}, open new: {}",
            self.open.chunk.chunk_id(),
            ChunkId(offset)
        );

        let state = get_state();

        let mut new_open = {
            let chunk_id = ChunkId(offset);
            OpenChunk::create(
                config,
                chunk_id,
                WALRecord::State(state.clone()),
            )?
        };

        std::mem::swap(&mut new_open, &mut self.open);
        self.flush_tx
            .send(FlushRequest::AppendFile(FileEntry::new(
                offset,
                new_open.chunk.f.clone(),
                state.last().cloned(),
            )))
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to send FlushRequest::AppendFile: {}", e),
                )
            })?;

        let chunk = new_open.chunk;
        let closed_id = chunk.chunk_id();
        let closed = ClosedChunk::new(chunk, state.clone());
        self.closed.insert(closed_id, closed);
        Ok(Some(state))
    }

    pub(crate) fn load_log_payload(
        &self,
        log_data: &LogData<T>,
    ) -> Result<T::LogPayload, io::Error> {
        let chunk_id = log_data.chunk_id;
        let segment = log_data.record_segment;

        // All logs in open chunk are cached.
        // See: payload_cache.set_last_evictable()

        let record = {
            let closed = self.closed.get(&chunk_id).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "Chunk not found: {}; when:(open cache-miss read)",
                        chunk_id
                    ),
                )
            })?;
            closed.chunk.read_record(segment)?
        };

        if let WALRecord::Append(log_id, payload) = record {
            debug_assert_eq!(log_id, log_data.log_id);
            Ok(payload)
        } else {
            panic!("Expect Record::Append but: {:?}", record);
        }
    }
}

impl<T> WAL<WALRecord<T>> for RaftLogWAL<T>
where T: Types
{
    fn append(&mut self, rec: &WALRecord<T>) -> Result<(), io::Error> {
        self.open.append_record(rec)?;
        Ok(())
    }

    fn last_segment(&self) -> Segment {
        self.open.chunk.last_segment()
    }
}
