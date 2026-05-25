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
            wal: chunked_wal::Config {
                dir: dir.to_string(),
                read_buffer_size,
                chunk_max_records,
                chunk_max_size,
                truncate_incomplete_record: None,
                flush_batch_wait: None,
                flush_batch_max_items: None,
            },
            log_cache_max_items,
            log_cache_capacity,
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
