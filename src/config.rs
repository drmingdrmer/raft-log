use std::format;

use crate::ChunkId;
use crate::errors::InvalidChunkFileName;
use crate::num;

/// Configuration for Raft-log.
///
/// This struct holds various configuration parameters for the Raft-log,
/// including directory location, cache settings, and chunk management options.
///
/// Optional parameters are `Option<T>` in this struct, and default values is
/// evaluated when a getter method is called.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Base directory for storing Raft-log files
    pub dir: String,

    /// Maximum number of items to keep in the log cache
    pub log_cache_max_items: Option<usize>,

    /// Maximum capacity of the log cache in bytes
    pub log_cache_capacity: Option<usize>,

    /// Size of the read buffer in bytes
    pub read_buffer_size: Option<usize>,

    /// Maximum number of records in a chunk
    pub chunk_max_records: Option<usize>,

    /// Maximum size of a chunk in bytes
    pub chunk_max_size: Option<usize>,

    /// Whether to truncate the last half sync-ed record.
    ///
    /// If truncate, the chunk is considered successfully opened.
    /// Otherwise, an io::Error will be returned.
    pub truncate_incomplete_record: Option<bool>,
}

impl Config {
    /// Creates a new Config with the specified directory and default values for
    /// other fields
    pub fn new(dir: impl ToString) -> Self {
        Self {
            dir: dir.to_string(),
            ..Default::default()
        }
    }

    /// Creates a new Config with all configurable parameters
    pub fn new_full(
        dir: impl ToString,
        log_cache_max_items: Option<usize>,
        log_cache_capacity: Option<usize>,
        read_buffer_size: Option<usize>,
        chunk_max_records: Option<usize>,
        chunk_max_size: Option<usize>,
    ) -> Self {
        Self {
            dir: dir.to_string(),
            log_cache_max_items,
            log_cache_capacity,
            read_buffer_size,
            chunk_max_records,
            chunk_max_size,
            truncate_incomplete_record: None,
        }
    }

    /// Returns the maximum number of items in log cache (defaults to 100,000)
    pub fn log_cache_max_items(&self) -> usize {
        self.log_cache_max_items.unwrap_or(100_000)
    }

    /// Returns the maximum capacity of log cache in bytes (defaults to 1GB)
    pub fn log_cache_capacity(&self) -> usize {
        self.log_cache_capacity.unwrap_or(1024 * 1024 * 1024)
    }

    /// Returns the size of read buffer in bytes (defaults to 64MB)
    pub fn read_buffer_size(&self) -> usize {
        self.read_buffer_size.unwrap_or(64 * 1024 * 1024)
    }

    /// Returns the maximum number of records per chunk (defaults to 1M records)
    pub fn chunk_max_records(&self) -> usize {
        self.chunk_max_records.unwrap_or(1024 * 1024)
    }

    /// Returns the maximum size of a chunk in bytes (defaults to 1GB)
    pub fn chunk_max_size(&self) -> usize {
        self.chunk_max_size.unwrap_or(1024 * 1024 * 1024)
    }

    /// Returns whether to truncate incomplete records (defaults to true)
    pub fn truncate_incomplete_record(&self) -> bool {
        self.truncate_incomplete_record.unwrap_or(true)
    }

    /// Returns the full path for a given chunk ID
    pub fn chunk_path(&self, chunk_id: ChunkId) -> String {
        let file_name = Self::chunk_file_name(chunk_id);
        format!("{}/{}", self.dir, file_name)
    }

    /// Generates the file name for a given chunk ID
    ///
    /// The file name format is "r-{padded_chunk_id}.wal"
    pub(crate) fn chunk_file_name(chunk_id: ChunkId) -> String {
        let file_name = num::format_pad_u64(*chunk_id);
        format!("r-{}.wal", file_name)
    }

    /// Parses a chunk file name and returns the chunk ID
    ///
    /// # Arguments
    /// * `file_name` - Name of the chunk file (format:
    ///   "r-{padded_chunk_id}.wal")
    ///
    /// # Returns
    /// * `Ok(u64)` - The chunk ID if parsing succeeds
    /// * `Err(InvalidChunkFileName)` - If the file name format is invalid
    pub(crate) fn parse_chunk_file_name(
        file_name: &str,
    ) -> Result<u64, InvalidChunkFileName> {
        // 1. Strip the ".wal" suffix or return an error if it's not there
        let without_suffix =
            file_name.strip_suffix(".wal").ok_or_else(|| {
                InvalidChunkFileName::new(file_name, "has no '.wal' suffix")
            })?;

        // 2. Strip the "r-" prefix or return an error if it's not there
        let without_prefix =
            without_suffix.strip_prefix("r-").ok_or_else(|| {
                InvalidChunkFileName::new(file_name, "has no 'r-' prefix")
            })?;

        if without_prefix.len() != 26 {
            return Err(InvalidChunkFileName::new(
                file_name,
                "does not have 26 digit after 'r-' prefix",
            ));
        }

        let digits = without_prefix
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>();

        // 3. Parse the remaining string as an u64
        digits.parse::<u64>().map_err(|e| {
            InvalidChunkFileName::new(
                file_name,
                format!("cannot parse as u64: {}", e),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn test_parse_chunk_file_name() {
        assert_eq!(
            Config::parse_chunk_file_name("r-10_100_000_000_001_200_000.wal"),
            Ok(10_100_000_000_001_200_000)
        );

        assert!(
            Config::parse_chunk_file_name("r-10_100_000_000_001_200_000_1.wal")
                .is_err()
        );
        assert!(Config::parse_chunk_file_name("r-1000000000.wal").is_err());
        assert!(
            Config::parse_chunk_file_name("r-10_100_000_000_001_200_000.wall")
                .is_err()
        );
        assert!(
            Config::parse_chunk_file_name("rrr-10_100_000_000_001_200_000.wal")
                .is_err()
        );
    }
}
