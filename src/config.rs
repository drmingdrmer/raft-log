/// Configuration for Raft-log.
///
/// This struct holds various configuration parameters for the Raft-log,
/// including WAL storage and cache settings.
///
/// Optional parameters are `Option<T>` in this struct, and default values is
/// evaluated when a getter method is called.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// WAL storage configuration.
    pub wal: chunked_wal::Config,

    /// Maximum number of items to keep in the log cache
    pub log_cache_max_items: Option<usize>,

    /// Maximum capacity of the log cache in bytes
    pub log_cache_capacity: Option<usize>,
}

impl Config {
    /// Creates a new Config with the specified directory and default values for
    /// other fields
    pub fn new(dir: impl ToString) -> Self {
        Self {
            wal: chunked_wal::Config::new(dir),
            ..Default::default()
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
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn test_explicit_config_embeds_wal_config() {
        let config = Config {
            wal: crate::chunked_wal::Config {
                dir: "raft-log-dir".to_string(),
                read_buffer_size: Some(13),
                chunk_max_records: Some(17),
                chunk_max_size: Some(19),
                truncate_incomplete_record: None,
                flush_batch_wait: None,
                flush_batch_max_items: None,
            },
            log_cache_max_items: Some(7),
            log_cache_capacity: Some(11),
        };

        assert_eq!("raft-log-dir", config.wal.dir);
        assert_eq!(Some(13), config.wal.read_buffer_size);
        assert_eq!(Some(17), config.wal.chunk_max_records);
        assert_eq!(Some(19), config.wal.chunk_max_size);
        assert_eq!(None, config.wal.truncate_incomplete_record);
        assert_eq!(None, config.wal.flush_batch_wait);
        assert_eq!(None, config.wal.flush_batch_max_items);
        assert_eq!(7, config.log_cache_max_items());
        assert_eq!(11, config.log_cache_capacity());
    }
}
