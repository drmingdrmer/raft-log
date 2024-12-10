use std::io;

use byteorder::BigEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use codeq::ChecksumReader;
use codeq::ChecksumWriter;

use crate::api::types::Types;
use crate::raft_log::state_machine::raft_log_state::RaftLogState;

/// WALRecord represents different types of records that can be written to the
/// Write-Ahead Log (WAL).
/// Each variant corresponds to a specific operation in the Raft protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WALRecord<T: Types> {
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

    /// Save a snapshot of the complete state of the Raft log.
    State(RaftLogState<T>),
}

impl<T: Types> WALRecord<T> {
    /// Returns the numeric type identifier for this record
    /// Used during encoding and decoding.
    pub(crate) fn record_type(&self) -> u32 {
        match self {
            WALRecord::SaveVote(_) => 0,
            WALRecord::Append(_, _) => 1,
            WALRecord::Commit(_) => 2,
            WALRecord::TruncateAfter(_) => 3,
            WALRecord::PurgeUpto(_) => 4,
            WALRecord::State(_) => 5,
        }
    }
}

/// Implements encoding for WALRecord
/// Each record is encoded as:
/// - 4 bytes: record type
/// - variable bytes: record payload
/// - 4 bytes: checksum
impl<T: Types> codeq::Encode for WALRecord<T> {
    fn encode<W: io::Write>(&self, mut w: W) -> Result<usize, io::Error> {
        let mut n = 0;
        let mut cw = ChecksumWriter::new(&mut w);

        // record type
        {
            let typ = self.record_type();
            cw.write_u32::<BigEndian>(typ)?;
            n += 4;
        }

        // record payload
        n += match self {
            WALRecord::SaveVote(vote) => vote.encode(&mut cw)?,
            WALRecord::Append(log_id, payload) => {
                log_id.encode(&mut cw)? + payload.encode(&mut cw)?
            }
            WALRecord::Commit(log_id) => log_id.encode(&mut cw)?,
            WALRecord::TruncateAfter(log_id) => log_id.encode(&mut cw)?,
            WALRecord::PurgeUpto(log_id) => log_id.encode(&mut cw)?,
            WALRecord::State(state) => state.encode(&mut cw)?,
        };

        // checksum
        n += cw.write_checksum()?;

        Ok(n)
    }
}

/// Implements decoding for WALRecord
/// Reads the record type, payload, and verifies the checksum
impl<T: Types> codeq::Decode for WALRecord<T> {
    fn decode<R: io::Read>(r: R) -> Result<Self, io::Error> {
        let mut cr = ChecksumReader::new(r);

        let record_type = cr.read_u32::<BigEndian>()?;

        let rec = match record_type {
            0 => Self::SaveVote(codeq::Decode::decode(&mut cr)?),
            1 => Self::Append(
                T::LogId::decode(&mut cr)?,
                T::LogPayload::decode(&mut cr)?,
            ),
            2 => Self::Commit(T::LogId::decode(&mut cr)?),
            3 => Self::TruncateAfter(Option::<T::LogId>::decode(&mut cr)?),
            4 => Self::PurgeUpto(T::LogId::decode(&mut cr)?),
            5 => Self::State(RaftLogState::decode(&mut cr)?),

            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unknown record type: {}", record_type),
                ));
            }
        };

        cr.verify_checksum(|| "Record::decode()")?;

        Ok(rec)
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use codeq::testing::test_codec;

    use crate::raft_log::state_machine::raft_log_state::RaftLogState;
    use crate::raft_log::wal::wal_record::WALRecord;
    use crate::testing::ss;
    use crate::testing::TestTypes;

    #[test]
    fn test_record_codec_vote() -> Result<(), io::Error> {
        let rec = WALRecord::<TestTypes>::SaveVote((1, 2));

        let b = vec![
            0, 0, 0, 0, // typ
            0, 0, 0, 0, 0, 0, 0, 1, // vote.term
            0, 0, 0, 0, 0, 0, 0, 2, // vote.voted_for
            0, 0, 0, 0, 246, 160, 238, 226, // checksum
        ];

        test_codec(&b, &rec)
    }

    #[test]
    fn test_record_codec_append() -> Result<(), io::Error> {
        let rec = WALRecord::<TestTypes>::Append((1, 2), ss("hello"));

        let b = vec![
            0, 0, 0, 1, // typ
            0, 0, 0, 0, 0, 0, 0, 1, // log_id.term
            0, 0, 0, 0, 0, 0, 0, 2, // log_id.index
            0, 0, 0, 5, // payload.len
            104, 101, 108, 108, 111, // payload
            0, 0, 0, 0, 167, 17, 197, 69, // checksum
        ];

        test_codec(&b, &rec)
    }

    #[test]
    fn test_record_codec_commit() -> Result<(), io::Error> {
        let rec = WALRecord::<TestTypes>::Commit((1, 2));

        let b = vec![
            0, 0, 0, 2, // typ
            0, 0, 0, 0, 0, 0, 0, 1, // commit.term
            0, 0, 0, 0, 0, 0, 0, 2, // commit.voted_for
            0, 0, 0, 0, 34, 156, 126, 37, // checksum
        ];

        test_codec(&b, &rec)
    }

    #[test]
    fn test_record_codec_truncate_after() -> Result<(), io::Error> {
        let rec = WALRecord::<TestTypes>::TruncateAfter(Some((1, 2)));

        let b = vec![
            0, 0, 0, 3, // typ
            1, // Some
            0, 0, 0, 0, 0, 0, 0, 1, // log_id.term
            0, 0, 0, 0, 0, 0, 0, 2, // log_id.index.
            0, 0, 0, 0, 213, 81, 166, 197, // checksum
        ];

        test_codec(&b, &rec)
    }

    #[test]
    fn test_record_codec_purge_upto() -> Result<(), io::Error> {
        let rec = WALRecord::<TestTypes>::PurgeUpto((1, 2));

        let b = vec![
            0, 0, 0, 4, // typ
            0, 0, 0, 0, 0, 0, 0, 1, // log_id.term
            0, 0, 0, 0, 0, 0, 0, 2, // log_id.index
            0, 0, 0, 0, 133, 168, 201, 45, // checksum
        ];

        test_codec(&b, &rec)
    }

    #[test]
    fn test_record_codec_state() -> Result<(), io::Error> {
        let rec = WALRecord::<TestTypes>::State(RaftLogState {
            vote: Some((1, 2)),
            last: Some((2, 3)),
            committed: Some((4, 5)),
            purged: Some((6, 7)),
            user_data: Some(ss("hello")),
        });

        let b = vec![
            0, 0, 0, 5, // typ
            1, // version
            1, // Some
            0, 0, 0, 0, 0, 0, 0, 1, // vote.term
            0, 0, 0, 0, 0, 0, 0, 2, // vote.voted_for
            1, // Some
            0, 0, 0, 0, 0, 0, 0, 2, // last.term
            0, 0, 0, 0, 0, 0, 0, 3, // last.index
            1, // Some
            0, 0, 0, 0, 0, 0, 0, 4, // committed.term
            0, 0, 0, 0, 0, 0, 0, 5, // committed.index
            1, // Some
            0, 0, 0, 0, 0, 0, 0, 6, // purged.term
            0, 0, 0, 0, 0, 0, 0, 7, // purged.index
            1, // Some
            0, 0, 0, 5, // user_data.len
            104, 101, 108, 108, 111, // user_data
            0, 0, 0, 0, 121, 106, 111, 41, // checksum
        ];

        test_codec(&b, &rec)
    }
}
