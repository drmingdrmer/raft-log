use std::io;
use std::sync::Arc;

use codeq::Segment;

use crate::chunk::Chunk;
use crate::dump_writer;
use crate::ChunkId;
use crate::Config;
use crate::RaftLog;
use crate::Types;
use crate::WALRecord;

pub struct Dump<T> {
    config: Arc<Config>,
    _p: std::marker::PhantomData<T>,
}

impl<T: Types> Dump<T> {
    pub(crate) fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            _p: std::marker::PhantomData,
        }
    }

    pub fn write_to_string(&self) -> Result<String, io::Error> {
        let mut buf = Vec::new();
        self.write(&mut buf)?;
        String::from_utf8(buf)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    pub fn write<W: io::Write>(&self, mut w: W) -> Result<(), io::Error> {
        writeln!(&mut w, "RaftLog:")?;
        self.write_with(w, dump_writer::multiline_string)
    }

    pub fn write_with<W: io::Write, D>(
        &self,
        mut w: W,
        write_record: D,
    ) -> Result<(), io::Error>
    where
        D: Fn(
            &mut W,
            ChunkId,
            u64,
            Result<(Segment, WALRecord<T>), io::Error>,
        ) -> Result<(), io::Error>,
    {
        let config = self.config.as_ref();
        let offsets = RaftLog::<T>::load_chunk_ids(config)?;
        for chunk_id in offsets {
            let it = Chunk::<T>::dump(config, chunk_id)?;
            for (i, res) in it.into_iter().enumerate() {
                write_record(&mut w, chunk_id, i as u64, res)?;
            }
        }
        Ok(())
    }
}
