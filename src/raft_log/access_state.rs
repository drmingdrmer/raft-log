use std::fmt;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

#[derive(Debug, Clone, Default)]
pub struct AccessStat {
    pub(crate) cache_hit: Arc<AtomicU64>,
    pub(crate) cache_miss: Arc<AtomicU64>,
}

impl fmt::Display for AccessStat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "AccessStat{{cache(hit/miss)={}/{}}}",
            self.cache_hit.load(Ordering::Relaxed),
            self.cache_miss.load(Ordering::Relaxed)
        )
    }
}
