use std::{collections::hash_map::DefaultHasher, hash::Hasher};

use super::*;

impl ShaderCache {
    pub fn new() -> Self {
        Self { index: 0, cache: [0; 4] }
    }

    pub fn insert(&mut self, source: &str) {
        let mut hash_builder = DefaultHasher::new();
        hash_builder.write(source.as_bytes());
        let hash = hash_builder.finish();
        self.cache[self.index as usize] = hash;
        self.index = (self.index + 1) % 4;
    }

    pub fn check(&self, source: &str) -> bool {
        let mut hash_builder = DefaultHasher::new();
        hash_builder.write(source.as_bytes());
        let hash = hash_builder.finish();
        self.cache[0] == hash || self.cache[1] == hash || self.cache[2] == hash || self.cache[3] == hash
    }
}
