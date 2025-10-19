use std::fmt;
use std::io;

use codeq::OffsetSize;

use crate::num::format_pad9_u64;
use crate::types::Segment;
use crate::ChunkId;
use crate::Types;
use crate::WALRecord;

pub fn write_record_debug<T: Types, W: io::Write>(
    w: &mut W,
    chunk_id: ChunkId,
    in_chunk_record_index: u64,
    res: Result<(Segment, WALRecord<T>), io::Error>,
) -> Result<(), io::Error> {
    match res {
        Ok((seg, rec)) => {
            // Before the first record of a chunk, print the chunk ID
            if seg.offset().0 == 0 {
                writeln!(w, "{}", chunk_id)?;
            }
            writeln!(
                w,
                "  R-{in_chunk_record_index:05}: [{}, {}) {}: {:?}",
                format_pad9_u64(*seg.offset()),
                format_pad9_u64(*seg.end()),
                seg.size(),
                rec
            )?;
        }
        Err(io_err) => {
            writeln!(w, "Error: {}", io_err)?;
        }
    }
    Ok(())
}

/// Write each record using `std::fmt::Display` instead of using
/// `std::fmt::Debug`
pub fn write_record_display<T: Types, W: io::Write>(
    w: &mut W,
    chunk_id: ChunkId,
    in_chunk_record_index: u64,
    res: Result<(Segment, WALRecord<T>), io::Error>,
) -> Result<(), io::Error>
where
    WALRecord<T>: fmt::Display,
{
    match res {
        Ok((seg, rec)) => {
            // Before the first record of a chunk, print the chunk ID
            if seg.offset().0 == 0 {
                writeln!(w, "{}", chunk_id)?;
            }
            writeln!(
                w,
                "  R-{in_chunk_record_index:05}: [{}, {}) {}: {}",
                format_pad9_u64(*seg.offset()),
                format_pad9_u64(*seg.end()),
                seg.size(),
                rec
            )?;
        }
        Err(io_err) => {
            writeln!(w, "Error: {}", io_err)?;
        }
    }
    Ok(())
}
