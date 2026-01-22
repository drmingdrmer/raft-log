use std::io;
use std::io::Cursor;

use rmp_serde::decode::Error as DecodeError;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TestData {
    id: u64,
    name: String,
    values: Vec<u32>,
}

/// Extract the io::ErrorKind from an rmp_serde decode error, if it wraps an IO
/// error.
fn get_io_error_kind(err: &DecodeError) -> Option<io::ErrorKind> {
    match err {
        DecodeError::InvalidMarkerRead(e) => Some(e.kind()),
        DecodeError::InvalidDataRead(e) => Some(e.kind()),
        _ => None,
    }
}

/// Test that `rmp_serde::decode::from_read()` returns an UnexpectedEof error
/// when the input reader contains truncated data.
#[test]
fn test_from_read_truncated_input() {
    let data = TestData {
        id: 12345,
        name: "hello world".to_string(),
        values: vec![1, 2, 3, 4, 5],
    };

    // Serialize the data
    let serialized = rmp_serde::to_vec(&data).unwrap();
    assert!(
        serialized.len() > 10,
        "Serialized data should be reasonably long"
    );

    // Test various truncation points
    for truncate_at in [1, 5, serialized.len() / 2, serialized.len() - 1] {
        let truncated = &serialized[..truncate_at];
        let mut reader = Cursor::new(truncated);

        let result: Result<TestData, _> =
            rmp_serde::decode::from_read(&mut reader);
        println!("error got:{:?}", result);

        assert!(
            result.is_err(),
            "Should fail with truncated data at {}",
            truncate_at
        );

        let err = result.unwrap_err();
        let kind = get_io_error_kind(&err);

        // The error should be an UnexpectedEof from the underlying IO
        assert_eq!(
            kind,
            Some(io::ErrorKind::UnexpectedEof),
            "Expected UnexpectedEof for truncation at {}, got: {:?}",
            truncate_at,
            err
        );
    }
}
