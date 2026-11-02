use std::collections::BTreeMap;

use log::info;

use crate::ChunkId;
use crate::Types;

/// A cached payload and the chunk that holds its `Append` record.
#[derive(Debug, Clone)]
pub(crate) struct CachedPayload<T: Types> {
    pub(crate) chunk_id: ChunkId,
    pub(crate) payload: T::LogPayload,
}

#[derive(Debug)]
pub(crate) struct PayloadCache<T: Types> {
    max_items: usize,
    capacity: usize,
    size: usize,
    pub(crate) cache: BTreeMap<T::LogId, CachedPayload<T>>,

    /// Every chunk before this one is closed and persisted.
    ///
    /// A payload may leave the cache only when its record is in such a chunk,
    /// because a cache miss reads the record back from a closed chunk only.
    /// All other payloads must be held in memory.
    ///
    /// The boundary is a chunk, not a log id, because a log id cannot tell an
    /// old record from its replacement: after a truncation, a replacement
    /// entry can sort below a log id that is already persisted while its own
    /// record still sits in the open chunk.
    evictable_before: ChunkId,
}

impl<T: Types> PayloadCache<T> {
    pub(crate) fn new(max_items: usize, capacity: usize) -> Self {
        Self {
            max_items,
            capacity,
            size: 0,
            cache: Default::default(),
            evictable_before: ChunkId(0),
        }
    }

    pub(crate) fn set_evictable_before(&mut self, chunk_id: ChunkId) {
        info!("RaftLog payload cache: set_evictable_before: {}", chunk_id);
        self.evictable_before = chunk_id;
    }

    pub(crate) fn evictable_before(&self) -> ChunkId {
        self.evictable_before
    }

    pub(crate) fn item_count(&self) -> usize {
        self.cache.len()
    }

    pub(crate) fn max_items(&self) -> usize {
        self.max_items
    }

    pub(crate) fn total_size(&self) -> usize {
        self.size
    }

    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    pub(crate) fn insert(
        &mut self,
        key: T::LogId,
        chunk_id: ChunkId,
        payload: T::LogPayload,
    ) {
        let payload_size = T::payload_size(&payload) as usize;

        self.cache.insert(key, CachedPayload { chunk_id, payload });
        self.size += payload_size;

        self.try_evict();
    }

    /// Evict from the smallest log id while the cache is over its limits.
    ///
    /// Stopping at the first entry that is not evictable is enough: a
    /// truncation drops every cached entry above the point it keeps, so the
    /// entries left are in log id order and in WAL order at once, and their
    /// chunk ids never decrease.
    pub(crate) fn try_evict(&mut self) {
        while self.need_evict() && self.first_is_evictable() {
            self.evict_first();
        }
    }

    /// Evict all evictable entries regardless of cache fullness.
    ///
    /// Normally, eviction only triggers during `insert()` when the cache
    /// exceeds `max_items` or `capacity`. Because `evictable_before` is set by
    /// the FlushWorker asynchronously, the number of items evicted during
    /// inserts depends on thread scheduling — making the cache size
    /// non-deterministic. This method forces a full eviction pass to bring the
    /// cache to a consistent state independent of timing.
    pub(crate) fn drain_evictable(&mut self) {
        while self.first_is_evictable() {
            self.evict_first();
        }
    }

    fn need_evict(&self) -> bool {
        self.cache.len() > self.max_items || self.size > self.capacity
    }

    fn first_is_evictable(&self) -> bool {
        self.cache
            .first_key_value()
            .is_some_and(|(_, cached)| self.is_evictable(cached))
    }

    fn is_evictable(&self, cached: &CachedPayload<T>) -> bool {
        cached.chunk_id < self.evictable_before
    }

    fn evict_first(&mut self) {
        if let Some((_log_id, cached)) = self.cache.pop_first() {
            self.size -= T::payload_size(&cached.payload) as usize;
        }
    }

    pub(crate) fn get(&self, key: &T::LogId) -> Option<T::LogPayload> {
        self.cache.get(key).map(|cached| cached.payload.clone())
    }

    pub(crate) fn truncate_after(&mut self, key: &T::LogId) {
        while let Some((log_id, cached)) = self.cache.pop_last() {
            if key < &log_id {
                self.size -= T::payload_size(&cached.payload) as usize;
            } else {
                self.cache.insert(log_id, cached);
                break;
            }
        }
    }

    pub(crate) fn purge_upto(&mut self, key: &T::LogId) {
        while let Some((log_id, cached)) = self.cache.pop_first() {
            if &log_id <= key && self.is_evictable(&cached) {
                self.size -= T::payload_size(&cached.payload) as usize;
            } else {
                self.cache.insert(log_id, cached);
                break;
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.cache.clear();
        self.size = 0;
    }
}

#[cfg(test)]
mod tests {
    use crate::ChunkId;
    use crate::raft_log::state_machine::payload_cache::PayloadCache;
    use crate::testing::TestTypes;

    #[test]
    fn test_cache() {
        let mut cache = PayloadCache::<TestTypes>::new(2, 10);
        // Make all evictable
        cache.set_evictable_before(ChunkId(1));

        let payload1 = "foo".to_string();
        let payload2 = "bar".to_string();
        let payload3 = "baz".to_string();
        let payload4 = "12345678".to_string();
        let payload5 = "123456789ab".to_string();

        cache.insert((1, 1), ChunkId(0), payload1.clone());
        cache.insert((1, 2), ChunkId(0), payload2.clone());
        assert_eq!(cache.item_count(), 2);
        assert_eq!(cache.total_size(), 6);

        assert_eq!(cache.get(&(1, 1)), Some(payload1.clone()));
        assert_eq!(cache.get(&(1, 2)), Some(payload2.clone()));

        // Evict early items by count

        cache.insert((1, 3), ChunkId(0), payload3.clone());
        assert_eq!(cache.item_count(), 2);
        assert_eq!(cache.total_size(), 6);

        assert_eq!(cache.get(&(1, 1)), None);
        assert_eq!(cache.get(&(1, 2)), Some(payload2.clone()));
        assert_eq!(cache.get(&(1, 3)), Some(payload3.clone()));

        // Evict early items by capacity

        cache.insert((1, 4), ChunkId(0), payload4.clone());
        assert_eq!(cache.item_count(), 1);
        assert_eq!(cache.total_size(), 8);

        assert_eq!(cache.get(&(1, 2)), None);
        assert_eq!(cache.get(&(1, 3)), None);
        assert_eq!(cache.get(&(1, 4)), Some(payload4.clone()));

        // Single item exceeds capacity

        cache.insert((1, 5), ChunkId(0), payload5.clone());
        assert_eq!(cache.item_count(), 0);
        assert_eq!(cache.total_size(), 0);

        assert_eq!(cache.get(&(1, 2)), None);
        assert_eq!(cache.get(&(1, 3)), None);
        assert_eq!(cache.get(&(1, 4)), None);
        assert_eq!(cache.get(&(1, 5)), None);
    }

    #[test]
    fn test_evictable_before() {
        let mut cache = PayloadCache::<TestTypes>::new(2, 10);
        cache.set_evictable_before(ChunkId(10));

        let payload1 = "foo".to_string();
        let payload2 = "bar".to_string();
        let payload3 = "baz".to_string();
        let payload4 = "12345678".to_string();
        let payload5 = "123456789ab".to_string();

        cache.insert((1, 1), ChunkId(0), payload1.clone());
        cache.insert((1, 2), ChunkId(0), payload2.clone());
        cache.insert((1, 3), ChunkId(10), payload3.clone());
        cache.insert((1, 4), ChunkId(10), payload4.clone());
        assert_eq!(cache.item_count(), 2);
        assert_eq!(cache.total_size(), 11);

        assert_eq!(cache.get(&(1, 1)), None);
        assert_eq!(cache.get(&(1, 2)), None);
        assert_eq!(cache.get(&(1, 3)), Some(payload3.clone()));
        assert_eq!(cache.get(&(1, 4)), Some(payload4.clone()));

        cache.insert((1, 5), ChunkId(10), payload5.clone());
        assert_eq!(cache.item_count(), 3);
        assert_eq!(cache.total_size(), 22);

        assert_eq!(cache.get(&(1, 1)), None);
        assert_eq!(cache.get(&(1, 2)), None);
        assert_eq!(cache.get(&(1, 3)), Some(payload3.clone()));
        assert_eq!(cache.get(&(1, 4)), Some(payload4.clone()));
        assert_eq!(cache.get(&(1, 5)), Some(payload5.clone()));
    }

    /// A replacement entry that sorts below persisted log ids stays cached
    /// while its own chunk is not persisted.
    #[test]
    fn test_replacement_below_persisted_log_id_is_not_evicted() {
        let mut cache = PayloadCache::<TestTypes>::new(0, 100);

        cache.insert((1, 0), ChunkId(0), "a".to_string());
        cache.insert((3, 1), ChunkId(0), "b".to_string());
        cache.insert((3, 2), ChunkId(0), "c".to_string());
        assert_eq!(cache.item_count(), 3);

        // Chunk 0 is persisted, chunk 10 is open.
        cache.set_evictable_before(ChunkId(10));

        cache.truncate_after(&(1, 0));
        cache.insert((2, 1), ChunkId(10), "replacement".to_string());

        assert_eq!(cache.item_count(), 1);
        assert_eq!(cache.get(&(1, 0)), None);
        assert_eq!(cache.get(&(2, 1)), Some("replacement".to_string()));

        cache.drain_evictable();
        assert_eq!(cache.get(&(2, 1)), Some("replacement".to_string()));
    }

    #[test]
    fn test_truncate_after() {
        let mut cache = PayloadCache::<TestTypes>::new(10, 100);

        let payload1 = "foo".to_string();
        let payload2 = "bar".to_string();
        let payload3 = "baz".to_string();
        let payload4 = "12345678".to_string();
        let payload5 = "123456789ab".to_string();

        cache.insert((1, 1), ChunkId(0), payload1.clone());
        cache.insert((1, 2), ChunkId(0), payload2.clone());
        cache.insert((2, 3), ChunkId(0), payload3.clone());
        cache.insert((2, 4), ChunkId(0), payload4.clone());
        cache.insert((2, 5), ChunkId(0), payload5.clone());

        cache.truncate_after(&(1, 3));
        assert_eq!(cache.item_count(), 2);
        assert_eq!(cache.total_size(), 6);

        assert_eq!(cache.get(&(1, 1)), Some(payload1.clone()));
        assert_eq!(cache.get(&(1, 2)), Some(payload2.clone()));
        assert_eq!(cache.get(&(2, 3)), None);
        assert_eq!(cache.get(&(2, 4)), None);
        assert_eq!(cache.get(&(2, 5)), None);

        cache.truncate_after(&(1, 2));
        assert_eq!(cache.item_count(), 2);
        assert_eq!(cache.total_size(), 6);

        assert_eq!(cache.get(&(1, 1)), Some(payload1.clone()));
        assert_eq!(cache.get(&(1, 2)), Some(payload2.clone()));

        cache.truncate_after(&(2, 3));
        assert_eq!(cache.item_count(), 2);
        assert_eq!(cache.total_size(), 6);

        assert_eq!(cache.get(&(1, 1)), Some(payload1.clone()));
        assert_eq!(cache.get(&(1, 2)), Some(payload2.clone()));

        cache.truncate_after(&(1, 1));
        assert_eq!(cache.item_count(), 1);
        assert_eq!(cache.total_size(), 3);

        assert_eq!(cache.get(&(1, 1)), Some(payload1.clone()));
        assert_eq!(cache.get(&(1, 2)), None);

        cache.truncate_after(&(1, 0));
        assert_eq!(cache.item_count(), 0);
        assert_eq!(cache.total_size(), 0);

        assert_eq!(cache.get(&(1, 1)), None);
        assert_eq!(cache.get(&(1, 2)), None);
    }

    #[test]
    fn test_purge_upto() {
        let mut cache = PayloadCache::<TestTypes>::new(10, 100);

        let payload1 = "foo".to_string();
        let payload2 = "bar".to_string();
        let payload3 = "baz".to_string();
        let payload4 = "12345678".to_string();
        let payload5 = "123456789ab".to_string();

        cache.insert((1, 1), ChunkId(0), payload1.clone());
        cache.insert((1, 2), ChunkId(0), payload2.clone());
        cache.insert((2, 3), ChunkId(10), payload3.clone());
        cache.insert((2, 4), ChunkId(20), payload4.clone());
        cache.insert((2, 5), ChunkId(30), payload5.clone());

        // Initial state
        assert_eq!(cache.item_count(), 5);
        assert_eq!(cache.total_size(), 28);

        // Purge up to (1, 2)
        cache.purge_upto(&(1, 2));
        assert_eq!(cache.item_count(), 5);

        cache.set_evictable_before(ChunkId(10));

        cache.purge_upto(&(1, 2));
        assert_eq!(cache.item_count(), 3);
        assert_eq!(cache.total_size(), 22);

        assert_eq!(cache.get(&(1, 1)), None);
        assert_eq!(cache.get(&(1, 2)), None);
        assert_eq!(cache.get(&(2, 3)), Some(payload3.clone()));
        assert_eq!(cache.get(&(2, 4)), Some(payload4.clone()));
        assert_eq!(cache.get(&(2, 5)), Some(payload5.clone()));

        // Purge up to (2, 3)
        cache.purge_upto(&(2, 3));
        assert_eq!(cache.item_count(), 3);

        cache.set_evictable_before(ChunkId(20));

        cache.purge_upto(&(2, 3));
        assert_eq!(cache.item_count(), 2);
        assert_eq!(cache.total_size(), 19);

        assert_eq!(cache.get(&(2, 3)), None);
        assert_eq!(cache.get(&(2, 4)), Some(payload4.clone()));
        assert_eq!(cache.get(&(2, 5)), Some(payload5.clone()));

        // Purge up to a key that doesn't exist but is between existing keys
        cache.set_evictable_before(ChunkId(30));
        cache.purge_upto(&(2, 4));
        assert_eq!(cache.item_count(), 1);
        assert_eq!(cache.total_size(), 11);

        assert_eq!(cache.get(&(2, 4)), None);
        assert_eq!(cache.get(&(2, 5)), Some(payload5.clone()));

        // Purge up to a key larger than all existing keys
        cache.set_evictable_before(ChunkId(40));
        cache.purge_upto(&(3, 0));
        assert_eq!(cache.item_count(), 0);
        assert_eq!(cache.total_size(), 0);

        assert_eq!(cache.get(&(2, 5)), None);
    }
}
