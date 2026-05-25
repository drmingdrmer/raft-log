use std::fmt;
use std::fmt::Formatter;
use std::io;

use byteorder::BigEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use chunked_wal::wal::wal_record::CHECKPOINT_RECORD_TYPE;
use codeq::Decode;
use codeq::Encode;
use codeq::config::CodeqConfig;
use display_more::DisplayOptionExt;

use crate::Types;
use crate::types::Checksum;

/// Raft log commands stored in the WAL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaftLogAction<T: Types> {
    /// Save vote change.
    SaveVote(T::Vote),

    /// Append new log entry.
    Append(T::LogId, T::LogPayload),

    /// Save committed log id.
    Commit(T::LogId),

    /// Truncate log entries after the specified log id.
    TruncateAfter(Option<T::LogId>),

    /// Purge log entries up to (and including) the specified log id.
    PurgeUpto(T::LogId),
}

impl<T: Types> RaftLogAction<T> {
    /// Returns the numeric type identifier for this action.
    pub(crate) fn record_type(&self) -> u32 {
        match self {
            RaftLogAction::SaveVote(_) => 0,
            RaftLogAction::Append(_, _) => 1,
            RaftLogAction::Commit(_) => 2,
            RaftLogAction::TruncateAfter(_) => 3,
            RaftLogAction::PurgeUpto(_) => 4,
        }
    }

    fn encode_payload<W: io::Write>(
        &self,
        mut w: W,
    ) -> Result<usize, io::Error> {
        match self {
            RaftLogAction::SaveVote(vote) => vote.encode(&mut w),
            RaftLogAction::Append(log_id, payload) => {
                Ok(log_id.encode(&mut w)? + payload.encode(&mut w)?)
            }
            RaftLogAction::Commit(log_id) => log_id.encode(&mut w),
            RaftLogAction::TruncateAfter(log_id) => log_id.encode(&mut w),
            RaftLogAction::PurgeUpto(log_id) => log_id.encode(&mut w),
        }
    }

    fn decode_payload<R: io::Read>(
        record_type: u32,
        mut r: R,
    ) -> Result<Self, io::Error> {
        match record_type {
            0 => Ok(Self::SaveVote(codeq::Decode::decode(&mut r)?)),
            1 => Ok(Self::Append(
                T::LogId::decode(&mut r)?,
                T::LogPayload::decode(&mut r)?,
            )),
            2 => Ok(Self::Commit(T::LogId::decode(&mut r)?)),
            3 => Ok(Self::TruncateAfter(Option::<T::LogId>::decode(&mut r)?)),
            4 => Ok(Self::PurgeUpto(T::LogId::decode(&mut r)?)),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown record type: {}", record_type),
            )),
        }
    }
}

impl<T> fmt::Display for RaftLogAction<T>
where
    T: Types,
    T::Vote: fmt::Display,
    T::LogId: fmt::Display,
    T::LogPayload: fmt::Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            RaftLogAction::SaveVote(vote) => {
                write!(f, "SaveVote({})", vote)
            }
            RaftLogAction::Append(log_id, payload) => {
                write!(f, "Append(log_id: {}, payload: {})", log_id, payload)
            }
            RaftLogAction::Commit(log_id) => {
                write!(f, "Commit({})", log_id)
            }
            RaftLogAction::TruncateAfter(option_log_id) => {
                write!(f, "TruncateAfter({})", option_log_id.display())
            }
            RaftLogAction::PurgeUpto(log_id) => {
                write!(f, "PurgeUpto({})", log_id)
            }
        }
    }
}

/// Implements encoding for WALRecord
/// Each record is encoded as:
/// - 4 bytes: record type
/// - variable bytes: record payload
/// - 4 bytes: checksum
impl<T: Types> codeq::Encode for RaftLogAction<T> {
    fn encode<W: io::Write>(&self, mut w: W) -> Result<usize, io::Error> {
        let mut n = 0;
        let mut cw = Checksum::new_writer(&mut w);

        // record type
        {
            let typ = self.record_type();
            if typ == CHECKPOINT_RECORD_TYPE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "Action record type {} is reserved for checkpoint",
                        CHECKPOINT_RECORD_TYPE
                    ),
                ));
            }
            cw.write_u32::<BigEndian>(typ)?;
            n += 4;
        }

        // record payload
        n += self.encode_payload(&mut cw)?;

        // checksum
        n += cw.write_checksum()?;

        Ok(n)
    }
}

impl<T: Types> codeq::Decode for RaftLogAction<T> {
    fn decode<R: io::Read>(r: R) -> Result<Self, io::Error> {
        let mut cr = Checksum::new_reader(r);

        let record_type = cr.read_u32::<BigEndian>()?;
        let rec = Self::decode_payload(record_type, &mut cr)?;

        cr.verify_checksum(|| "Record::decode()")?;

        Ok(rec)
    }
}
