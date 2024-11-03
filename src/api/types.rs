use std::fmt::Debug;

use codeq::Codec;

use crate::raft_log::wal::callback::Callback;

pub trait Types
where Self: Debug + Default + PartialEq + Eq + Clone + 'static
{
    type LogId: Debug + Clone + Ord + Eq + Codec + Send + Sync + 'static;
    type LogPayload: Debug + Clone + Codec + Send + Sync + 'static;
    type Vote: Debug + Clone + PartialOrd + Eq + Codec + 'static;
    type Callback: Callback + Send + 'static;

    type UserData: Debug + Clone + Eq + Codec + 'static;

    fn log_index(log_id: &Self::LogId) -> u64;

    fn payload_size(payload: &Self::LogPayload) -> u64;

    fn next_log_index(log_id: Option<&Self::LogId>) -> u64 {
        match log_id {
            Some(log_id) => Self::log_index(log_id) + 1,
            None => 0,
        }
    }
}
