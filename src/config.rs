use std::format;

use crate::errors::InvalidChunkFileName;
use crate::num;
use crate::ChunkId;

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub dir: String,

    pub log_cache_max_items: Option<usize>,

    pub log_cache_capacity: Option<usize>,

    pub read_buffer_size: Option<usize>,

    /// Maximum number of records in a chunk.
    pub chunk_max_records: Option<usize>,

    pub chunk_max_size: Option<usize>,

    /// Whether to truncate the last half sync-ed record.
    pub(crate) truncate_incomplete_record: Option<bool>,
}

impl Config {
    pub fn new(dir: impl ToString) -> Self {
        Self {
            dir: dir.to_string(),
            ..Default::default()
        }
    }

    pub fn log_cache_max_items(&self) -> usize {
        self.log_cache_max_items.unwrap_or(100_000)
    }

    pub fn log_cache_capacity(&self) -> usize {
        self.log_cache_capacity.unwrap_or(1024 * 1024 * 1024)
    }

    pub fn read_buffer_size(&self) -> usize {
        self.read_buffer_size.unwrap_or(64 * 1024 * 1024)
    }

    pub fn chunk_max_records(&self) -> usize {
        self.chunk_max_records.unwrap_or(1024 * 1024)
    }

    pub fn chunk_max_size(&self) -> usize {
        self.chunk_max_size.unwrap_or(1024 * 1024 * 1024)
    }

    pub fn truncate_incomplete_record(&self) -> bool {
        self.truncate_incomplete_record.unwrap_or(true)
    }

    pub fn chunk_path(&self, chunk_id: ChunkId) -> String {
        let file_name = Self::chunk_file_name(chunk_id);
        format!("{}/{}", self.dir, file_name)
    }

    pub(crate) fn chunk_file_name(chunk_id: ChunkId) -> String {
        let file_name = num::format_pad_u64(*chunk_id);
        format!("r-{}.wal", file_name)
    }

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

        assert!(Config::parse_chunk_file_name(
            "r-10_100_000_000_001_200_000_1.wal"
        )
        .is_err());
        assert!(Config::parse_chunk_file_name("r-1000000000.wal").is_err());
        assert!(Config::parse_chunk_file_name(
            "r-10_100_000_000_001_200_000.wall"
        )
        .is_err());
        assert!(Config::parse_chunk_file_name(
            "rrr-10_100_000_000_001_200_000.wal"
        )
        .is_err());
    }
}
