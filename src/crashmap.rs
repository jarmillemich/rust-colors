use std::{sync::{RwLock, atomic::{Ordering, AtomicIsize}}, hash::Hasher, collections::BTreeMap};
use fnv::FnvHasher;

type BinType<K, V> = RwLock<BTreeMap<K, V>>;

/// An atomic addition/removal HashMap with very weak guarantees!
pub struct CrashMap<K: core::hash::Hash + Ord, V> {
    bins: Box<[BinType<K, V>]>,
    bin_scale: u8,
    count: AtomicIsize
}

impl<K: core::hash::Hash + Ord + Clone + Copy, V> CrashMap<K, V> {
    pub fn with_capacity(capacity: usize) -> CrashMap<K, V> {
        let mut bin_scale = 0;
        while 1 << bin_scale < capacity { bin_scale += 1; }
        let capacity_actual = 1 << bin_scale;
        assert!(capacity_actual >= capacity, "Calculated capacity should be big enough");

        CrashMap {
            bins: (0..capacity_actual).map(|_| RwLock::new(BTreeMap::new())).collect::<Vec<_>>().into_boxed_slice(),
            bin_scale,
            count: AtomicIsize::default(),
        }
    }

    pub fn get_capacity(&self) -> usize {
        1 << self.bin_scale
    }

    pub fn len(&self) -> usize {
        self.count.load(Ordering::Relaxed) as usize
    }

    fn get_bin<'a>(&'a self, key: K) -> &'a BinType<K, V> {
        let mut hasher = FnvHasher::default();
        key.hash(&mut hasher);
        let key_hash = hasher.finish();
        let bin_idx = key_hash as usize & (self.get_capacity() - 1);
        &self.bins[bin_idx]
    }

    pub fn contains_key(&self, key: K) -> bool {
        let bin = self.get_bin(key);
        bin.read().unwrap().contains_key(&key)
    }

    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let bin = self.get_bin(key);
        let mut writer = bin.write().unwrap();
        self.count.fetch_add(1, Ordering::Relaxed);
        writer.insert(key, value)
    }

    pub fn remove(&self, key: K) -> Option<V> {
        let bin = self.get_bin(key);
        let mut writer = bin.write().unwrap();
        self.count.fetch_add(-1, Ordering::Relaxed);
        writer.remove(&key)
    }

    pub fn foreach_lockfree<F: FnMut((&K, &V)) -> ()>(&self, mut f: F) -> () {
        for bin in self.bins.iter() {
            // Try and get a read lock. If not, just carry on
            if let Ok(lock) = bin.try_read() {
                for item in lock.iter() {
                    f(item);
                }
            }
        }
    }
}

pub struct CrashSet<K: core::hash::Hash + Ord + Clone + Copy> {
    map: CrashMap<K, ()>
}

impl<K: core::hash::Hash + Ord + Clone + Copy> CrashSet<K> {
    pub fn with_capacity(capacity: usize) -> CrashSet<K> {
        CrashSet { map: CrashMap::with_capacity(capacity) }
    }

    pub fn get_capacity(&self) -> usize { self.map.get_capacity() }
    pub fn len(&self) -> usize { self.map.len() }
    pub fn contains(&self, key: K) -> bool { self.map.contains_key(key) }
    pub fn insert(&self, key: K) -> bool {
        self.map.insert(key, ()) != None
    }
    pub fn remove(&self, key: K) -> bool {
        self.map.remove(key) != None
    }

    pub fn foreach_lockfree<F: FnMut(&K) -> ()>(&self, mut f: F) {
        self.map.foreach_lockfree(|(&k, &_)| f(&k));
    }
}

#[test]
fn test_map_contains_key() {
    let map = CrashMap::with_capacity(1024);
    map.insert(16, 32);

    assert_eq!(map.len(), 1);
    assert!(map.contains_key(16));
    assert!(!map.contains_key(17));

    map.insert(17, 213);

    assert!(map.contains_key(17));
}

#[test]
fn test_map_iter() {
    // Note, full retrieval is guaranteed ONLY when single threaded
    // In a multi-threaded environment, iter may skip bins that are locked
    let map = CrashMap::with_capacity(1024);
    map.insert(16, -16);
    map.insert(17, -17);
    map.insert(18, -18);

    assert_eq!(map.len(), 3);

    let mut results = vec![];

    map.foreach_lockfree(|(&k, &v)| {
        results.push((k, v));
    });

    assert_eq!(map.len(), 3);

    assert_eq!(results.len(), 3);
    assert!(results.contains(&(16, -16)));
    assert!(results.contains(&(17, -17)));
    assert!(results.contains(&(18, -18)));
}

#[test]
fn test_set_contains() {
    let set = CrashSet::with_capacity(1024);
    set.insert(1);
    set.insert(7);
    set.insert(13);

    assert_eq!(set.len(), 3);

    assert!(set.contains(1));
    assert!(set.contains(7));
    assert!(set.contains(13));

    assert!(!set.contains(-1));
    assert!(!set.contains(0));
    assert!(!set.contains(17));
}