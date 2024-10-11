#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(thiserror::Error)]
#[error("Invalid WAL chunk file name: {bad_file_name}: {reason}")]
pub struct InvalidChunkFileName {
    pub bad_file_name: String,
    pub reason: String,
}

impl InvalidChunkFileName {
    pub fn new(bad_file_name: impl ToString, reason: impl ToString) -> Self {
        Self {
            bad_file_name: bad_file_name.to_string(),
            reason: reason.to_string(),
        }
    }
}
