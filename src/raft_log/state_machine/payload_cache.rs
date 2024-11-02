use std::collections::BTreeMap;

use crate::Types;

#[derive(Debug)]
pub(crate) struct PayloadCache<T: Types> {
    max_items: usize,
    capacity: usize,
    size: usize,
    cache: BTreeMap<T::LogId, T::LogPayload>,

    /// The last evictable log id.
    ///
    /// All logs with log id after this value must be held in memory.
    last_evictable: Option<T::LogId>,
}

impl<T: Types> PayloadCache<T> {
    pub(crate) fn new(max_items: usize, capacity: usize) -> Self {
        Self {
            max_items,
            capacity,
            size: 0,
            cache: Default::default(),
            last_evictable: None,
        }
    }

    pub(crate) fn set_last_evictable(&mut self, log_id: Option<T::LogId>) {
        println!("set_last_evictable: {:?}", log_id);
        self.last_evictable = log_id;
    }

    #[allow(dead_code)]
    pub(crate) fn item_count(&self) -> usize {
        self.cache.len()
    }

    #[allow(dead_code)]
    pub(crate) fn total_size(&self) -> usize {
        self.size
    }

    pub(crate) fn insert(&mut self, key: T::LogId, value: T::LogPayload) {
        let payload_size = T::payload_size(&value) as usize;

        self.cache.insert(key, value);
        self.size += payload_size;

        self.try_evict();
    }

    pub(crate) fn try_evict(&mut self) {
        if self.last_evictable.is_some() {
            while self.need_evict() {
                if let Some((log_id, _payload)) = self.cache.first_key_value() {
                    if Some(log_id) <= self.last_evictable.as_ref() {
                        self.evict_first()
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
        } else {
            while self.need_evict() {
                self.evict_first();
            }
        }
    }

    fn need_evict(&self) -> bool {
        self.cache.len() > self.max_items || self.size > self.capacity
    }

    fn evict_first(&mut self) {
        if let Some((_log_id, payload)) = self.cache.pop_first() {
            self.size -= T::payload_size(&payload) as usize;
        }
    }

    pub(crate) fn get(&self, key: &T::LogId) -> Option<T::LogPayload> {
        self.cache.get(key).cloned()
    }

    pub(crate) fn truncate_after(&mut self, key: &T::LogId) {
        while let Some((log_id, payload)) = self.cache.pop_last() {
            if key < &log_id {
                self.size -= T::payload_size(&payload) as usize;
            } else {
                self.cache.insert(log_id, payload);
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
    use crate::raft_log::state_machine::payload_cache::PayloadCache;
    use crate::testing::TestTypes;

    #[test]
    fn test_cache() {
        let mut cache = PayloadCache::<TestTypes>::new(2, 10);

        let payload1 = "foo".to_string();
        let payload2 = "bar".to_string();
        let payload3 = "baz".to_string();
        let payload4 = "12345678".to_string();
        let payload5 = "123456789ab".to_string();

        cache.insert((1, 1), payload1.clone());
        cache.insert((1, 2), payload2.clone());
        assert_eq!(cache.item_count(), 2);
        assert_eq!(cache.total_size(), 6);

        assert_eq!(cache.get(&(1, 1)), Some(payload1.clone()));
        assert_eq!(cache.get(&(1, 2)), Some(payload2.clone()));

        // Evict early items by count

        cache.insert((1, 3), payload3.clone());
        assert_eq!(cache.item_count(), 2);
        assert_eq!(cache.total_size(), 6);

        assert_eq!(cache.get(&(1, 1)), None);
        assert_eq!(cache.get(&(1, 2)), Some(payload2.clone()));
        assert_eq!(cache.get(&(1, 3)), Some(payload3.clone()));

        // Evict early items by capacity

        cache.insert((1, 4), payload4.clone());
        assert_eq!(cache.item_count(), 1);
        assert_eq!(cache.total_size(), 8);

        assert_eq!(cache.get(&(1, 2)), None);
        assert_eq!(cache.get(&(1, 3)), None);
        assert_eq!(cache.get(&(1, 4)), Some(payload4.clone()));

        // Single item exceeds capacity

        cache.insert((1, 5), payload5.clone());
        assert_eq!(cache.item_count(), 0);
        assert_eq!(cache.total_size(), 0);

        assert_eq!(cache.get(&(1, 2)), None);
        assert_eq!(cache.get(&(1, 3)), None);
        assert_eq!(cache.get(&(1, 4)), None);
        assert_eq!(cache.get(&(1, 5)), None);
    }

    #[test]
    fn test_last_evictable() {
        let mut cache = PayloadCache::<TestTypes>::new(2, 10);
        cache.set_last_evictable(Some((1, 2)));

        let payload1 = "foo".to_string();
        let payload2 = "bar".to_string();
        let payload3 = "baz".to_string();
        let payload4 = "12345678".to_string();
        let payload5 = "123456789ab".to_string();

        cache.insert((1, 1), payload1.clone());
        cache.insert((1, 2), payload2.clone());
        cache.insert((1, 3), payload3.clone());
        cache.insert((1, 4), payload4.clone());
        assert_eq!(cache.item_count(), 2);
        assert_eq!(cache.total_size(), 11);

        assert_eq!(cache.get(&(1, 1)), None);
        assert_eq!(cache.get(&(1, 2)), None);
        assert_eq!(cache.get(&(1, 3)), Some(payload3.clone()));
        assert_eq!(cache.get(&(1, 4)), Some(payload4.clone()));

        cache.insert((1, 5), payload5.clone());
        assert_eq!(cache.item_count(), 3);
        assert_eq!(cache.total_size(), 22);

        assert_eq!(cache.get(&(1, 1)), None);
        assert_eq!(cache.get(&(1, 2)), None);
        assert_eq!(cache.get(&(1, 3)), Some(payload3.clone()));
        assert_eq!(cache.get(&(1, 4)), Some(payload4.clone()));
        assert_eq!(cache.get(&(1, 5)), Some(payload5.clone()));
    }

    #[test]
    fn test_truncate_after() {
        let mut cache = PayloadCache::<TestTypes>::new(10, 100);

        let payload1 = "foo".to_string();
        let payload2 = "bar".to_string();
        let payload3 = "baz".to_string();
        let payload4 = "12345678".to_string();
        let payload5 = "123456789ab".to_string();

        cache.insert((1, 1), payload1.clone());
        cache.insert((1, 2), payload2.clone());
        cache.insert((2, 3), payload3.clone());
        cache.insert((2, 4), payload4.clone());
        cache.insert((2, 5), payload5.clone());

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
}
