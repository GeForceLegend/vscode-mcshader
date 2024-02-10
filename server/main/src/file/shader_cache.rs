use std::{collections::hash_map::DefaultHasher, hash::Hasher};

use super::*;

impl ShaderCache {
    pub fn new() -> Self {
        Self {
            index: 0,
            cache: [(0, String::new()), (0, String::new()), (0, String::new()), (0, String::new())],
        }
    }

    pub fn insert(&mut self, source: String) {
        let mut hash_builder = DefaultHasher::new();
        hash_builder.write(source.as_bytes());
        let hash = hash_builder.finish();
        self.cache[self.index as usize] = (hash, source);
        self.index = (self.index + 1) % 4;
    }

    pub fn check(&self, source: &str) -> bool {
        let mut hash_builder = DefaultHasher::new();
        hash_builder.write(source.as_bytes());
        let hash = hash_builder.finish();
        self.cache[self.index as usize].0 == hash
            || self.cache[(self.index as usize + 3) % 4].0 == hash
            || self.cache[(self.index as usize + 2) % 4].0 == hash
            || self.cache[(self.index as usize + 1) % 4].0 == hash
    }
}
