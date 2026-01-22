use std::any::type_name;
use std::fmt::Debug;
use std::io;

use codeq::Codec;

use crate::api::types::Types;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
pub(crate) struct TestTypes;

impl Types for TestTypes {
    /// (term, index)
    type LogId = (u64, u64);

    type LogPayload = String;
    /// (term, voted_for)
    type Vote = (u64, u64);

    type Callback = std::sync::mpsc::SyncSender<Result<(), io::Error>>;

    type UserData = String;

    fn log_index(log_id: &Self::LogId) -> u64 {
        log_id.1
    }

    fn payload_size(payload: &Self::LogPayload) -> u64 {
        payload.len() as u64
    }
}

/// Type config to test Display implementation
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
pub(crate) struct TestDisplayTypes;

impl Types for TestDisplayTypes {
    /// (term, index)
    type LogId = u64;

    type LogPayload = String;
    /// (term, voted_for)
    type Vote = u64;

    type Callback = std::sync::mpsc::SyncSender<Result<(), io::Error>>;

    type UserData = String;

    fn log_index(log_id: &Self::LogId) -> u64 {
        *log_id
    }

    fn payload_size(payload: &Self::LogPayload) -> u64 {
        payload.len() as u64
    }
}

#[allow(dead_code)]
pub fn test_codec_without_corruption<D: Codec + PartialEq + Debug>(
    encoded_bytes: &[u8],
    v: &D,
) -> Result<(), io::Error> {
    // convert `correct` to string if possible
    let correct_str = String::from_utf8_lossy(encoded_bytes);
    println!("correct data: {}", correct_str);

    let mes =
        format!("Type: {} encoded data: {:?}", type_name::<D>(), correct_str);

    // Test encoding
    {
        let mut b = Vec::new();
        let n = v.encode(&mut b)?;
        assert_eq!(n, b.len(), "output len, {}", &mes);
        assert_eq!(b, encoded_bytes, "output data, {}", &mes);
    }

    // Assert the input is correct

    {
        let b = encoded_bytes.to_vec();
        let decoded = D::decode(&mut b.as_slice())?;
        assert_eq!(v, &decoded, "decode, {}", &mes);
    }

    Ok(())
}

/// Create a string
#[allow(dead_code)]
pub(crate) fn ss(x: impl ToString) -> String {
    x.to_string()
}
