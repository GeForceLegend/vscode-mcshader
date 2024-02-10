use std::{collections::hash_map::DefaultHasher, hash::Hasher};

use super::*;

impl ShaderCache {
    pub fn new() -> Self {
        Self { index: 0, cache: [0; 8] }
    }

    pub fn insert(&mut self, source: &str) {
        let mut hash_builder = DefaultHasher::new();
        hash_builder.write(source.as_bytes());
        let hash = hash_builder.finish();
        self.cache[self.index as usize] = hash;
        self.index = (self.index + 1) % 8;
    }

    pub fn check(&self, source: &str) -> bool {
        let mut hash_builder = DefaultHasher::new();
        hash_builder.write(source.as_bytes());
        let hash = hash_builder.finish();
        self.cache.contains(&hash)
    }
}
