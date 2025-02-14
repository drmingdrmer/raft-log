pub use codeq::config::Crc32fast;

/// This crate use Crc32fast checksum.
pub type Checksum = Crc32fast;

pub type Segment = codeq::Segment<Crc32fast>;
