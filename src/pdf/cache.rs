//! Byte-budgeted LRU for immutable, shared values. Keys identify content snapshots.

use std::{collections::VecDeque, sync::Arc};

struct Entry<K, V> {
    key: K,
    value: Arc<V>,
    bytes: usize,
}

pub struct ByteLru<K, V> {
    entries: VecDeque<Entry<K, V>>,
    bytes: usize,
    capacity: usize,
}

impl<K: Eq, V> ByteLru<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            capacity,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<Arc<V>> {
        let index = self.entries.iter().position(|entry| entry.key == *key)?;
        let entry = self.entries.remove(index)?;
        let value = Arc::clone(&entry.value);
        self.entries.push_back(entry);
        Some(value)
    }

    /// Check residency without changing the LRU order.
    pub fn contains(&self, key: &K) -> bool {
        self.entries.iter().any(|entry| entry.key == *key)
    }

    /// Keep the original Arc for duplicate keys and mark it recently used.
    /// A single oversized value is ignored without evicting resident entries.
    /// Measure the immutable payload only on a cache miss; the callback is not stored.
    pub fn insert(&mut self, key: K, value: Arc<V>, measure_bytes: impl FnOnce(&V) -> usize) {
        if self.get(&key).is_some() {
            return;
        }
        let bytes = measure_bytes(&value);
        if bytes > self.capacity {
            return;
        }
        // Evict before accounting for the new entry, also avoiding overflow
        // when a caller supplies a capacity close to usize::MAX.
        while self.bytes > self.capacity - bytes {
            if let Some(removed) = self.entries.pop_front() {
                self.bytes -= removed.bytes;
            }
        }
        self.bytes += bytes;
        self.entries.push_back(Entry { key, value, bytes });
    }

    #[cfg(test)]
    pub fn used_bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_eviction_promotes_reads_but_not_presence_checks() {
        let mut cache = ByteLru::new(8);
        let first = Arc::new("first");
        // Consuming captured weights also verifies that the callback may be FnOnce.
        let weights = vec![2, 2];
        cache.insert(1, Arc::clone(&first), move |_| weights.into_iter().sum());
        cache.insert(2, Arc::new("second"), |_| 3);
        assert!(Arc::ptr_eq(&cache.get(&1).unwrap(), &first));
        assert!(cache.contains(&2));
        cache.insert(3, Arc::new("third"), |_| 2);
        assert!(cache.contains(&1));
        assert!(!cache.contains(&2));
        assert!(cache.contains(&3));
        assert_eq!(cache.used_bytes(), 6);
    }

    #[test]
    fn duplicate_key_keeps_the_original_arc_weight_and_recency() {
        let mut cache = ByteLru::new(8);
        let first = Arc::new("first");
        cache.insert(1, Arc::clone(&first), |_| 4);
        cache.insert(2, Arc::new("second"), |_| 4);
        cache.insert(1, Arc::new("duplicate"), |_| {
            panic!("duplicate keys must not measure the new value")
        });
        assert_eq!(cache.used_bytes(), 8);
        cache.insert(3, Arc::new("third"), |_| 4);
        assert!(!cache.contains(&2));
        assert!(Arc::ptr_eq(&cache.get(&1).unwrap(), &first));
        assert_eq!(cache.used_bytes(), 8);
    }

    #[test]
    fn oversized_values_preserve_residents_and_exact_budget_is_accepted() {
        let mut cache = ByteLru::new(8);
        let first = Arc::new("first");
        cache.insert(1, Arc::clone(&first), |_| 4);
        cache.insert(2, Arc::new("too large"), |_| 9);
        assert_eq!(cache.used_bytes(), 4);
        assert!(!cache.contains(&2));
        assert!(Arc::ptr_eq(&cache.get(&1).unwrap(), &first));
        cache.insert(3, Arc::new("exact budget"), |_| 8);
        assert!(!cache.contains(&1));
        assert!(cache.contains(&3));
        assert_eq!(cache.used_bytes(), 8);
    }
}
