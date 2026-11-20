use alloc::vec::Vec;
use core::hash::{Hash,Hasher};


const BUCKET_COUNT: usize = 1024;

struct SimpleHasher {
    state: u64,
}

impl Hasher for SimpleHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= byte as u64;
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }
    fn finish(&self) -> u64 {
        self.state
    }
}

pub struct HashMap<K, V> {
    buckets: Vec<Vec<(K, V)>>,
}

impl<K, V> HashMap<K, V> {
    pub fn new() -> Self {
        let mut buckets = Vec::with_capacity(BUCKET_COUNT);

        for _ in 0..BUCKET_COUNT {
            buckets.push(Vec::new());
        }

        Self { buckets }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> + '_ {
        self.buckets
            .iter()
            .flat_map(|bucket| bucket.iter())
            .map(|(key, value)| (key, value))
    }
}

impl <K: Hash, V> HashMap<K, V> {
    fn bucket_index(&self, key: &K) -> usize {
        let mut hasher = SimpleHasher {
            state: 0xcbf29ce484222325,
        };

        key.hash(&mut hasher);

        (hasher.finish() % self.buckets.len() as u64) as usize
    }
    
}

impl<K: Hash + Eq, V> HashMap<K, V> {
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let index = self.bucket_index(&key);
        let bucket = &mut self.buckets[index];

        for (stored_key, stored_value) in bucket.iter_mut() {
            if *stored_key == key {
                let old_value = core::mem::replace(stored_value, value);
                return Some(old_value);
            }
        }

        bucket.push((key, value));
        None
    }
}

