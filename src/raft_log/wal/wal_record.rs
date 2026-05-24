use std::fmt;
use std::fmt::Formatter;
use std::io;
use std::io::Cursor;
use std::io::Read;

use byteorder::BigEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use codeq::config::CodeqConfig;

use crate::WalTypes;
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
pub enum WALRecord<W>
where W: WalTypes
{
    /// A user-defined command.
    Action(W::Action),

    /// A state-machine checkpoint persisted by the WAL.
    Checkpoint(W::Checkpoint),
}

impl<W> fmt::Debug for WALRecord<W>
where W: WalTypes
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

impl<W> fmt::Display for WALRecord<W>
where
    W: WalTypes,
    W::Action: fmt::Display,
    W::Checkpoint: fmt::Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WALRecord::Action(action) => fmt::Display::fmt(action, f),
            WALRecord::Checkpoint(checkpoint) => {
                write!(f, "Checkpoint({})", checkpoint)
            }
        }
    }
}

impl<W> codeq::Encode for WALRecord<W>
where W: WalTypes
{
    fn encode<Wt: io::Write>(&self, mut w: Wt) -> Result<usize, io::Error> {
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
impl<W> codeq::Decode for WALRecord<W>
where W: WalTypes
{
    fn decode<R: io::Read>(mut r: R) -> Result<Self, io::Error> {
        let mut type_bytes = [0; 4];
        r.read_exact(&mut type_bytes)?;

        if u32::from_be_bytes(type_bytes) != CHECKPOINT_RECORD_TYPE {
            let mut r = Cursor::new(type_bytes).chain(r);
            return Ok(Self::Action(W::Action::decode(&mut r)?));
        }

        let mut cr = Checksum::new_reader(Cursor::new(type_bytes).chain(r));
        cr.read_u32::<BigEndian>()?;
        let rec = Self::Checkpoint(W::Checkpoint::decode(&mut cr)?);
        cr.verify_checksum(|| "Record::decode()")?;

        Ok(rec)
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::mpsc::SyncSender;

    use codeq::Decode;
    use codeq::Encode;

    use crate::WalTypes;
    use crate::raft_log::wal::wal_record::CHECKPOINT_RECORD_TYPE;
    use crate::raft_log::wal::wal_record::WALRecord;

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    struct TestWal;

    impl WalTypes for TestWal {
        type Action = String;
        type Checkpoint = String;
        type Callback = SyncSender<Result<(), io::Error>>;
    }

    fn action(v: &str) -> WALRecord<TestWal> {
        WALRecord::Action(v.to_string())
    }

    fn checkpoint(v: &str) -> WALRecord<TestWal> {
        WALRecord::Checkpoint(v.to_string())
    }

    fn checkpoint_state_bytes() -> Vec<u8> {
        vec![
            0, 0, 0, 5, // checkpoint record type
            0, 0, 0, 5, // checkpoint string len
            115, 116, 97, 116, 101, // checkpoint string: state
            0, 0, 0, 0, 220, 33, 57, 147, // checksum
        ]
    }

    #[test]
    fn test_action_debug_display_and_clone() {
        let rec = action("vote");

        assert_eq!("\"vote\"", format!("{:?}", rec));
        assert_eq!("vote", format!("{}", rec));
        assert_eq!(rec, rec.clone());
    }

    #[test]
    fn test_checkpoint_debug_display_and_clone() {
        let rec = checkpoint("state");

        assert_eq!("State(\"state\")", format!("{:?}", rec));
        assert_eq!("Checkpoint(state)", format!("{}", rec));
        assert_eq!(rec, rec.clone());
    }

    #[test]
    fn test_encode_action_delegates_to_action_codec() -> Result<(), io::Error> {
        let mut got = Vec::new();

        let n = action("vote").encode(&mut got)?;

        assert_eq!(got.len(), n);
        assert_eq!(vec![0, 0, 0, 4, 118, 111, 116, 101], got);
        Ok(())
    }

    #[test]
    fn test_encode_checkpoint_adds_type_and_checksum() -> Result<(), io::Error>
    {
        let mut got = Vec::new();

        let n = checkpoint("state").encode(&mut got)?;

        assert_eq!(CHECKPOINT_RECORD_TYPE, 5);
        assert_eq!(got.len(), n);
        assert_eq!(checkpoint_state_bytes(), got);
        Ok(())
    }

    #[test]
    fn test_decode_action_replays_record_type_bytes() -> Result<(), io::Error> {
        let mut bytes = Vec::new();
        action("vote").encode(&mut bytes)?;
        action("log").encode(&mut bytes)?;

        let mut r = &bytes[..];

        assert_eq!(action("vote"), WALRecord::<TestWal>::decode(&mut r)?);
        assert_eq!(action("log"), WALRecord::<TestWal>::decode(&mut r)?);
        assert_eq!(&[] as &[u8], r);
        Ok(())
    }

    #[test]
    fn test_decode_checkpoint_verifies_checksum() -> Result<(), io::Error> {
        let bytes = checkpoint_state_bytes();

        let got = WALRecord::<TestWal>::decode(&mut bytes.as_slice())?;

        assert_eq!(checkpoint("state"), got);
        Ok(())
    }

    #[test]
    fn test_decode_checkpoint_rejects_bad_checksum() {
        let mut bytes = checkpoint_state_bytes();
        *bytes.last_mut().unwrap() ^= 1;

        let err = WALRecord::<TestWal>::decode(&mut bytes.as_slice())
            .expect_err("corrupted checkpoint checksum must fail");

        assert_eq!(io::ErrorKind::InvalidData, err.kind());
        assert!(err.to_string().contains("Record::decode()"));
    }

    #[test]
    fn test_decode_rejects_short_record_type() {
        let err = WALRecord::<TestWal>::decode(&mut [0, 0, 0].as_slice())
            .expect_err("short record type must fail");

        assert_eq!(io::ErrorKind::UnexpectedEof, err.kind());
    }
}
