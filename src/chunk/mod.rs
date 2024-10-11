pub(crate) mod chunk_id;
pub(crate) mod closed_chunk;
pub(crate) mod open_chunk;
mod record_iterator;

use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Seek;
use std::marker::PhantomData;
use std::sync::Arc;

use codeq::error_context_ext::ErrorContextExt;
use codeq::Decode;
use codeq::OffsetSize;
use codeq::Segment;
use record_iterator::RecordIterator;

use crate::chunk::chunk_id::ChunkId;
use crate::Config;
use crate::Types;
use crate::WALRecord;

#[derive(Debug)]
pub struct Chunk<T> {
    pub(crate) f: Arc<File>,

    /// The **global** offset of each record in the file.
    ///
    /// There are N.O. records + 1 offsets. The last one is the length of the
    /// file.
    ///
    /// Global offset means the offset since the first chunk, not this chunk.
    pub(crate) global_offsets: Vec<u64>,

    pub(crate) _p: PhantomData<T>,
}

impl<T> Chunk<T> {
    pub(crate) fn records_count(&self) -> usize {
        self.global_offsets.len() - 1
    }

    pub(crate) fn chunk_id(&self) -> ChunkId {
        ChunkId(self.global_offsets[0])
    }

    pub(crate) fn last_segment(&self) -> Segment {
        let offsets = &self.global_offsets;
        let l = offsets.len();

        let start = offsets[l - 2];
        let end = offsets[l - 1];

        Segment::new(start, end - start)
    }

    pub(crate) fn end_offset(&self) -> u64 {
        self.global_offsets[self.global_offsets.len() - 1]
            - self.global_offsets[0]
    }

    pub(crate) fn global_start(&self) -> u64 {
        self.global_offsets[0]
    }

    #[allow(dead_code)]
    pub(crate) fn global_end(&self) -> u64 {
        self.global_offsets[self.global_offsets.len() - 1]
    }

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
            .context(format_args!("open {}", chunk_id))?;

        Ok(f)
    }
}

impl<T> Chunk<T>
where T: Types
{
    pub(crate) fn open(
        config: Arc<Config>,
        chunk_id: ChunkId,
    ) -> Result<(Self, Vec<WALRecord<T>>), io::Error> {
        let mut f = Self::open_chunk_file(&config, chunk_id)?;
        let it = Self::load_records_iter(&config, &mut f, chunk_id)?;

        let mut record_offsets = vec![chunk_id.offset()];
        let mut records = Vec::new();
        let mut truncate = false;

        for res in it {
            match res {
                Ok((seg, record)) => {
                    record_offsets.push(chunk_id.offset() + seg.end());
                    records.push(record);
                }
                Err(io_err) => {
                    if io_err.kind() == io::ErrorKind::UnexpectedEof {
                        // Incomplete record, discard it and all the following
                        // records.
                        truncate = config.truncate_incomplete_record();
                        if truncate {
                            break;
                        }
                    }

                    return Err(io_err);
                }
            };
        }

        if truncate {
            f.set_len(*record_offsets.last().unwrap() - chunk_id.offset())?;
            f.sync_all()?;
        }

        let chunk = Self {
            f: Arc::new(f),
            global_offsets: record_offsets,
            _p: Default::default(),
        };

        Ok((chunk, records))
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn dump(
        config: &Config,
        chunk_id: ChunkId,
    ) -> Result<Vec<Result<(Segment, WALRecord<T>), io::Error>>, io::Error>
    {
        // When dumping, do not truncate file:
        let mut config = config.clone();
        config.truncate_incomplete_record = Some(false);

        let mut f = Self::open_chunk_file(&config, chunk_id)?;
        let it = Self::load_records_iter(&config, &mut f, chunk_id)?;

        Ok(it.collect::<Vec<_>>())
    }

    /// Returns an iterator of `start, end, record` or error.
    pub(crate) fn load_records_iter<'a>(
        config: &'a Config,
        f: &'a mut File,
        chunk_id: ChunkId,
    ) -> Result<
        impl Iterator<Item = Result<(Segment, WALRecord<T>), io::Error>> + 'a,
        io::Error,
    > {
        let file_size = f
            .seek(io::SeekFrom::End(0))
            .context(format_args!("seek end of {chunk_id}"))?;
        f.seek(io::SeekFrom::Start(0))
            .context(format_args!("seek start of {chunk_id}"))?;

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

        WALRecord::<T>::decode(br).context(format_args!(
            "decode Record {:?} in {}",
            segment,
            self.chunk_id()
        ))
    }
}
