use std::{cell::RefCell, path::PathBuf};

use hashbrown::{HashMap, HashSet};
use tree_sitter::Parser;

use crate::file::{IncludeFile, ShaderFile, TempFile};

use super::ServerData;

impl ServerData {
    pub fn new(parser: Parser) -> Self {
        ServerData {
            extensions: RefCell::new(HashSet::new()),
            shader_packs: RefCell::new(HashSet::new()),
            shader_files: RefCell::new(HashMap::new()),
            include_files: RefCell::new(HashMap::new()),
            temp_files: RefCell::new(HashMap::new()),
            tree_sitter_parser: RefCell::new(parser),
        }
    }

    pub fn shader_files(&self) -> &RefCell<HashMap<PathBuf, ShaderFile>> {
        &self.shader_files
    }

    pub fn include_files(&self) -> &RefCell<HashMap<PathBuf, IncludeFile>> {
        &self.include_files
    }

    pub fn temp_files(&self) -> &RefCell<HashMap<PathBuf, TempFile>> {
        &self.temp_files
    }
}
