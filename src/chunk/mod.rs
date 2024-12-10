//! Manages the creation, opening, and management of log chunks.
//!
//! A chunk is a segment of the Write-Ahead Log (WAL) that contains a sequence
//! of records. Chunks are used to:
//! - Break down large logs into manageable pieces
//! - Enable efficient record lookup and iteration
//! - Support log truncation and cleanup
//!
//! Each chunk maintains its position in the global log using absolute offsets,
//! which allows for consistent addressing regardless of chunk boundaries.

pub(crate) mod chunk_id;
pub(crate) mod closed_chunk;
pub(crate) mod open_chunk;
mod record_iterator;

use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::BufReader;
use std::io::Read;
use std::io::Seek;
use std::marker::PhantomData;
use std::sync::Arc;

use codeq::error_context_ext::ErrorContextExt;
use codeq::Decode;
use codeq::OffsetSize;
use codeq::Segment;
use log::error;
use log::warn;
use record_iterator::RecordIterator;

use crate::chunk::chunk_id::ChunkId;
use crate::Config;
use crate::Types;
use crate::WALRecord;

/// Represents a chunk of the Write-Ahead Log containing a sequence of records.
///
/// A chunk maintains:
/// - A file handle for persistent storage
/// - Global offsets for all records it contains
/// - Metadata about its position in the complete log
#[derive(Debug, Clone)]
pub struct Chunk<T> {
    /// File handle for the chunk's persistent storage
    pub(crate) f: Arc<File>,

    /// The global offsets of each record in the file.
    ///
    /// Contains N+1 offsets where N is the number of records:
    /// - First offset is the chunk's starting position
    /// - Last offset is the end of the last record
    /// - Offsets are absolute positions in the complete log, not relative to
    ///   chunk start
    pub(crate) global_offsets: Vec<u64>,

    /// Records the original file size if the chunk was truncated due to an
    /// incomplete write.
    ///
    /// This field is primarily used for testing and debugging purposes.
    #[allow(dead_code)]
    pub(crate) truncated: Option<u64>,

    pub(crate) _p: PhantomData<T>,
}

impl<T> Chunk<T> {
    /// Returns the number of records stored in this chunk.
    pub(crate) fn records_count(&self) -> usize {
        self.global_offsets.len() - 1
    }

    /// Returns this chunk's globally unique identifier.
    pub(crate) fn chunk_id(&self) -> ChunkId {
        ChunkId(self.global_offsets[0])
    }

    /// Returns the segment representing the last record in this chunk.
    pub(crate) fn last_segment(&self) -> Segment {
        let offsets = &self.global_offsets;
        let l = offsets.len();

        let start = offsets[l - 2];
        let end = offsets[l - 1];

        Segment::new(start, end - start)
    }

    /// Returns the total size of this chunk in bytes.
    pub(crate) fn chunk_size(&self) -> u64 {
        self.end_offset()
    }

    /// Returns the size of this chunk in bytes, calculated as the difference
    /// between its end and start offsets.
    #[allow(dead_code)]
    pub(crate) fn end_offset(&self) -> u64 {
        self.global_offsets[self.global_offsets.len() - 1]
            - self.global_offsets[0]
    }

    /// Returns the global offset where this chunk begins.
    pub(crate) fn global_start(&self) -> u64 {
        self.global_offsets[0]
    }

    /// Returns the global offset where this chunk ends.
    #[allow(dead_code)]
    pub(crate) fn global_end(&self) -> u64 {
        self.global_offsets[self.global_offsets.len() - 1]
    }

    /// Appends the size of a new record to the global offsets list.
    pub(crate) fn append_record_size(&mut self, size: u64) {
        let last = self.global_offsets[self.global_offsets.len() - 1];
        self.global_offsets.push(last + size);
    }

    pub(crate) fn open_chunk_file(
        config: &Config,
        chunk_id: ChunkId,
    ) -> Result<File, io::Error> {
        let path = config.chunk_path(chunk_id);
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .context(|| format!("open {}", chunk_id))?;

        Ok(f)
    }
}

impl<T> Chunk<T>
where T: Types
{
    /// Opens a chunk and loads its records.
    ///
    /// This function performs the following steps:
    /// 1. Opens the chunk file
    /// 2. Loads the records from the file
    /// 3. Verifies the integrity of the records
    pub(crate) fn open(
        config: Arc<Config>,
        chunk_id: ChunkId,
    ) -> Result<(Self, Vec<WALRecord<T>>), io::Error> {
        let f = Self::open_chunk_file(&config, chunk_id)?;
        let arc_f = Arc::new(f);
        let file_size = arc_f.metadata()?.len();
        let it = Self::load_records_iter(&config, arc_f.clone(), chunk_id)?;

        let mut record_offsets = vec![chunk_id.offset()];
        let mut records = Vec::new();
        let mut truncate = false;
        let mut truncated = None;

        for res in it {
            match res {
                Ok((seg, record)) => {
                    record_offsets.push(chunk_id.offset() + seg.end());
                    records.push(record);
                }
                Err(io_err) => {
                    let global_offset = record_offsets.last().copied().unwrap();

                    if io_err.kind() == io::ErrorKind::UnexpectedEof {
                        // Incomplete record, discard it and all the following
                        // records.
                        truncate = config.truncate_incomplete_record();
                        if truncate {
                            break;
                        }
                    } else {
                        // Maybe damaged or unfinished write with trailing
                        // zeros.
                        //
                        // Trailing zeros can happen if EXT4 is mounted with
                        // `data=writeback` mode, with which, data
                        // and metadata(file len) will be written to disk in
                        // arbitrary order.

                        let all_zero = Self::verify_trailing_zeros(
                            arc_f.clone(),
                            global_offset - chunk_id.offset(),
                            chunk_id,
                        )?;

                        if all_zero {
                            warn!(
                                "Trailing zeros detected at {} in chunk {}; Treat it as unfinished write",
                                global_offset,
                                chunk_id
                            );
                            truncate = config.truncate_incomplete_record();
                            if truncate {
                                break;
                            }
                        } else {
                            error!("Found damaged bytes: {}", io_err);
                        }
                    }

                    return Err(io_err);
                }
            };
        }

        if truncate {
            arc_f
                .set_len(*record_offsets.last().unwrap() - chunk_id.offset())?;
            arc_f.sync_all()?;
            truncated = Some(file_size);
        }

        let chunk = Self {
            f: arc_f,
            global_offsets: record_offsets,
            truncated,
            _p: Default::default(),
        };

        Ok((chunk, records))
    }

    /// Checks if a file contains only zero bytes from a specified offset to the
    /// end.
    ///
    /// This function is used to detect and validate partially written or
    /// corrupted data. It reads the file in chunks and verifies that all
    /// bytes after the given offset are zeros. This is particularly useful
    /// for detecting incomplete or interrupted writes where the remaining
    /// space may have been zero-filled.
    fn verify_trailing_zeros(
        mut file: Arc<File>,
        mut start_offset: u64,
        chunk_id: ChunkId,
    ) -> Result<bool, io::Error> {
        let file_size = file.metadata()?.len();

        if start_offset > file_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Start offset {} exceeds file size {}",
                    start_offset, file_size
                ),
            ));
        }

        if file_size == start_offset {
            return Ok(true);
        }

        const WARN_THRESHOLD: u64 = 64 * 1024; // 64KB
        if file_size - start_offset > WARN_THRESHOLD {
            warn!(
                "Large maybe damaged section detected: {} bytes to the end; in chunk {}",
                file_size - start_offset,
                chunk_id
            );
        }

        file.seek(io::SeekFrom::Start(start_offset))?;

        const BUFFER_SIZE: usize = 16 * 1024 * 1024; // 16MB
        const READ_CHUNK_SIZE: usize = 1024; // 1KB
        let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
        let mut buffer = vec![0; READ_CHUNK_SIZE];

        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }

            for (i, byt) in buffer.iter().enumerate().take(n) {
                if *byt != 0 {
                    error!(
                        "Non-zero byte detected at offset {} in chunk {}",
                        start_offset + i as u64,
                        chunk_id
                    );
                    return Ok(false);
                }
            }

            start_offset += n as u64;
        }
        Ok(true)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn dump(
        config: &Config,
        chunk_id: ChunkId,
    ) -> Result<Vec<Result<(Segment, WALRecord<T>), io::Error>>, io::Error>
    {
        let f = Self::open_chunk_file(config, chunk_id)?;
        let it = Self::load_records_iter(config, Arc::new(f), chunk_id)?;

        Ok(it.collect::<Vec<_>>())
    }

    /// Returns an iterator of `start, end, record` or error.
    ///
    /// This method should always use a newly opened file.
    /// Because `seek` may affect other open file descriptors.
    pub(crate) fn load_records_iter(
        config: &Config,
        mut f: Arc<File>,
        chunk_id: ChunkId,
    ) -> Result<
        impl Iterator<Item = Result<(Segment, WALRecord<T>), io::Error>> + '_,
        io::Error,
    > {
        let file_size = f
            .seek(io::SeekFrom::End(0))
            .context(|| format!("seek end of {chunk_id}"))?;
        f.seek(io::SeekFrom::Start(0))
            .context(|| format!("seek start of {chunk_id}"))?;

        let br = io::BufReader::with_capacity(config.read_buffer_size(), f);
        Ok(RecordIterator::new(br, file_size, chunk_id))
    }

    pub(crate) fn read_record(
        &self,
        segment: Segment,
    ) -> Result<WALRecord<T>, io::Error> {
        let mut f = self.f.as_ref();
        f.seek(io::SeekFrom::Start(segment.offset() - self.global_start()))?;

        let br = io::BufReader::with_capacity(16 * 1024, f);

        WALRecord::<T>::decode(br).context(|| {
            format!("decode Record {:?} in {}", segment, self.chunk_id())
        })
    }
}
