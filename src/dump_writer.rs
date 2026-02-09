use std::fmt;
use std::io;

use codeq::OffsetSize;

use crate::ChunkId;
use crate::Types;
use crate::WALRecord;
use crate::num::format_pad9_u64;
use crate::types::Segment;

struct DebugAsDisplay<'a, T: fmt::Debug>(&'a T);

impl<T: fmt::Debug> fmt::Display for DebugAsDisplay<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.0, f)
    }
}

pub fn write_record_debug<T: Types, W: io::Write>(
    w: &mut W,
    chunk_id: ChunkId,
    in_chunk_record_index: u64,
    res: Result<(Segment, WALRecord<T>), io::Error>,
) -> Result<(), io::Error> {
    match res {
        Ok((seg, rec)) => write_ok(
            w,
            chunk_id,
            in_chunk_record_index,
            &seg,
            &DebugAsDisplay(&rec),
        ),
        Err(e) => writeln!(w, "Error: {}", e),
    }
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
            write_ok(w, chunk_id, in_chunk_record_index, &seg, &rec)
        }
        Err(e) => writeln!(w, "Error: {}", e),
    }
}

fn write_ok<W: io::Write>(
    w: &mut W,
    chunk_id: ChunkId,
    in_chunk_record_index: u64,
    seg: &Segment,
    rec: &dyn fmt::Display,
) -> Result<(), io::Error> {
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
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestDisplayTypes;

    fn make_input() -> Result<(Segment, WALRecord<TestDisplayTypes>), io::Error>
    {
        Ok((
            Segment::new(0, 10),
            WALRecord::Append(3, "hello".to_string()),
        ))
    }

    #[test]
    fn test_write_record_debug_vs_display() {
        let mut debug_buf = Vec::new();
        write_record_debug(&mut debug_buf, ChunkId(0), 0, make_input())
            .unwrap();
        let debug_out = String::from_utf8(debug_buf).unwrap();

        let mut display_buf = Vec::new();
        write_record_display(&mut display_buf, ChunkId(0), 0, make_input())
            .unwrap();
        let display_out = String::from_utf8(display_buf).unwrap();

        // Debug uses derived format: Append(3, "hello")
        assert!(
            debug_out.contains(r#"Append(3, "hello")"#),
            "got: {debug_out}"
        );
        // Display uses manual format: Append(log_id: 3, payload: hello)
        assert!(
            display_out.contains("Append(log_id: 3, payload: hello)"),
            "got: {display_out}"
        );
    }
}
