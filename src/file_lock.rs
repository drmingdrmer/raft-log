use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::sync::Arc;

use fs2::FileExt;

use crate::Config;

#[derive(Debug)]
pub(crate) struct FileLock {
    f: File,
}

impl FileLock {
    pub(crate) fn new(config: Arc<Config>) -> Result<Self, io::Error> {
        let path = Self::lock_path(config.as_ref());

        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        f.try_lock_exclusive().map_err(|e| {
            println!("Error: {:?}", e);
            io::Error::new(
                io::ErrorKind::WouldBlock,
                format!(
                    "Directory '{}' is already locked by another process, \
                    shutdown other process to continue; \
                    error:({})",
                    config.dir, e
                ),
            )
        })?;

        Ok(Self { f })
    }

    pub(crate) fn lock_path(config: &Config) -> String {
        format!("{}/{}", config.dir, "LOCK")
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.f.unlock();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::file_lock::FileLock;
    use crate::Config;

    #[test]
    fn test_lock_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let p = temp_dir.path();
        let p = p.to_str().unwrap().to_string();

        let config = Config {
            dir: p,
            ..Default::default()
        };

        let config = Arc::new(config);

        let lf = FileLock::new(config.clone()).unwrap();
        println!("Directory locked successfully");

        let lf2 = FileLock::new(config.clone());
        assert!(lf2.is_err());

        drop(lf);
        let _lf2 = FileLock::new(config.clone()).unwrap();
        println!("Directory locked successfully after dropping first lock");
    }
}
