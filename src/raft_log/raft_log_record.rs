use crate::RaftWalTypes;
use crate::WALRecord;

/// Concrete WAL record used by `RaftLog`.
///
/// Its codec intentionally keeps the v1 on-disk tags:
/// action tags use `0..=4`, and checkpoint uses `5`.
pub type RaftLogRecord<T> = WALRecord<RaftWalTypes<T>>;

#[cfg(test)]
mod tests {
    use std::io;

    use codeq::Decode;
    use codeq::Encode;
    use codeq::testing::test_codec;
    use display_more::DisplaySliceExt;

    use crate::RaftLogRecord;
    use crate::raft_log::raft_log_action::RaftLogAction;
    use crate::raft_log::state_machine::raft_log_state::RaftLogState;
    use crate::testing::TestDisplayTypes;
    use crate::testing::TestTypes;
    use crate::testing::ss;

    #[test]
    fn test_record_codec_vote() -> Result<(), io::Error> {
        let rec =
            RaftLogRecord::<TestTypes>::Action(RaftLogAction::SaveVote((1, 2)));

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
        let rec = RaftLogRecord::<TestTypes>::Action(RaftLogAction::Append(
            (1, 2),
            ss("hello"),
        ));

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
        let rec =
            RaftLogRecord::<TestTypes>::Action(RaftLogAction::Commit((1, 2)));

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
        let rec = RaftLogRecord::<TestTypes>::Action(
            RaftLogAction::TruncateAfter(Some((1, 2))),
        );

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
        let rec = RaftLogRecord::<TestTypes>::Action(RaftLogAction::PurgeUpto(
            (1, 2),
        ));

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
        let rec = RaftLogRecord::<TestTypes>::Checkpoint(RaftLogState {
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

    #[test]
    fn test_decode_action_does_not_consume_next_record() -> Result<(), io::Error>
    {
        let record_1 = RaftLogRecord::<TestTypes>::Action(
            RaftLogAction::TruncateAfter(None),
        );
        let record_2 =
            RaftLogRecord::<TestTypes>::Action(RaftLogAction::Commit((3, 4)));

        let mut b = Vec::new();
        record_1.encode(&mut b)?;
        record_2.encode(&mut b)?;

        let mut r = &b[..];
        assert_eq!(record_1, RaftLogRecord::<TestTypes>::decode(&mut r)?);
        assert_eq!(record_2, RaftLogRecord::<TestTypes>::decode(&mut r)?);
        assert_eq!(&[] as &[u8], r);

        Ok(())
    }

    #[test]
    fn test_wal_record_display() {
        let records = [
            RaftLogRecord::<TestDisplayTypes>::Action(RaftLogAction::SaveVote(
                1,
            )),
            RaftLogRecord::<TestDisplayTypes>::Action(RaftLogAction::Append(
                3u64,
                "hello".to_string(),
            )),
            RaftLogRecord::<TestDisplayTypes>::Action(RaftLogAction::Commit(
                5u64,
            )),
            RaftLogRecord::<TestDisplayTypes>::Action(
                RaftLogAction::TruncateAfter(Some(7u64)),
            ),
            RaftLogRecord::<TestDisplayTypes>::Action(
                RaftLogAction::PurgeUpto(9u64),
            ),
            RaftLogRecord::<TestDisplayTypes>::Checkpoint(RaftLogState {
                vote: Some(1),
                last: Some(3),
                committed: Some(4),
                purged: Some(6),
                user_data: Some("hello".to_string()),
            }),
        ];

        let got = format!("{}", records.display_n(1000));

        let want = "[SaveVote(1),Append(log_id: 3, payload: hello),Commit(5),TruncateAfter(7),PurgeUpto(9),Checkpoint(RaftLogState(vote: 1, last: 3, committed: 4, purged: 6, user_data: hello))]";

        assert_eq!(want, got);
    }
}
