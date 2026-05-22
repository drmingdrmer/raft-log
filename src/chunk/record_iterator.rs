use std::io;
use std::marker::PhantomData;

use codeq::Decode;
use codeq::error_context_ext::ErrorContextExt;

use crate::ChunkId;
use crate::offset_reader::OffsetReader;
use crate::types::Segment;

pub(crate) struct RecordIterator<R, Rec> {
    r: OffsetReader<R>,
    total_size: u64,
    chunk_id: ChunkId,
    error: Option<io::Error>,
    _p: PhantomData<Rec>,
}

impl<R, Rec> RecordIterator<R, Rec>
where R: io::Read
{
    pub(crate) fn new(r: R, size: u64, chunk_id: ChunkId) -> Self {
        Self {
            r: OffsetReader::new(r),
            total_size: size,
            chunk_id,
            error: None,
            _p: Default::default(),
        }
    }
}

impl<R, Rec> Iterator for RecordIterator<R, Rec>
where
    R: io::Read,
    Rec: Decode,
{
    type Item = Result<(Segment, Rec), io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.error.is_some() {
            return None;
        }

        let start = self.r.offset();
        if start as u64 == self.total_size {
            return None;
        }

        let r = Rec::decode(&mut self.r);

        let res = r
            .map(|r| {
                (
                    Segment::new(
                        start as u64,
                        (self.r.offset() - start) as u64,
                    ),
                    r,
                )
            })
            .context(|| format!("decode Record at offset {}", start))
            .context(|| format!("iterate {}", self.chunk_id));

        if let Err(ref e) = res {
            self.error = Some(io::Error::new(e.kind(), e.to_string()));
        }

        Some(res)
    }
}
