use std::fmt;
use std::fmt::Formatter;
use std::io;
use std::io::Cursor;
use std::io::Read;

use byteorder::BigEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use codeq::config::CodeqConfig;

use crate::types::Checksum;

/// For historical reasons and compatibility, the WAL reserves record types
/// `0..=4` for user actions, and `5` for checkpoints.
pub(crate) const CHECKPOINT_RECORD_TYPE: u32 = 5;

/// Generic record stored in the Write-Ahead Log (WAL).
///
/// The WAL only distinguishes user actions from state-machine checkpoints.
/// The concrete action and checkpoint payloads are defined by the user of the
/// WAL.
#[derive(Clone, PartialEq, Eq)]
pub enum WALRecord<Act, Chkp> {
    /// A user-defined command.
    Action(Act),

    /// A state-machine checkpoint persisted by the WAL.
    Checkpoint(Chkp),
}

impl<Act, Chkp> fmt::Debug for WALRecord<Act, Chkp>
where
    Act: fmt::Debug,
    Chkp: fmt::Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WALRecord::Action(action) => fmt::Debug::fmt(action, f),
            WALRecord::Checkpoint(checkpoint) => {
                f.debug_tuple("State").field(checkpoint).finish()
            }
        }
    }
}

impl<Act, Chkp> codeq::Encode for WALRecord<Act, Chkp>
where
    Act: codeq::Encode,
    Chkp: codeq::Encode,
{
    fn encode<W: io::Write>(&self, mut w: W) -> Result<usize, io::Error> {
        match self {
            WALRecord::Action(action) => action.encode(&mut w),
            WALRecord::Checkpoint(checkpoint) => {
                let mut n = 0;
                let mut cw = Checksum::new_writer(&mut w);

                cw.write_u32::<BigEndian>(CHECKPOINT_RECORD_TYPE)?;
                n += 4;

                n += checkpoint.encode(&mut cw)?;
                n += cw.write_checksum()?;

                Ok(n)
            }
        }
    }
}

/// Implements decoding for WALRecord.
///
/// The wrapper inspects the record type and replays it for the decoder.
/// Checkpoint records reread the reserved checkpoint type so v1 checksum
/// verification still covers the type and payload.
impl<Act, Chkp> codeq::Decode for WALRecord<Act, Chkp>
where
    Act: codeq::Decode,
    Chkp: codeq::Decode,
{
    fn decode<R: io::Read>(mut r: R) -> Result<Self, io::Error> {
        let mut type_bytes = [0; 4];
        r.read_exact(&mut type_bytes)?;

        if u32::from_be_bytes(type_bytes) != CHECKPOINT_RECORD_TYPE {
            let mut r = Cursor::new(type_bytes).chain(r);
            return Ok(Self::Action(Act::decode(&mut r)?));
        }

        let mut cr = Checksum::new_reader(Cursor::new(type_bytes).chain(r));
        cr.read_u32::<BigEndian>()?;
        let rec = Self::Checkpoint(Chkp::decode(&mut cr)?);
        cr.verify_checksum(|| "Record::decode()")?;

        Ok(rec)
    }
}
