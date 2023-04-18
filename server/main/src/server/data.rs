use std::{cell::RefCell, path::PathBuf};

use hashbrown::{HashMap, HashSet};
use tree_sitter::Parser;

use crate::file::*;

use super::ServerData;

impl ServerData {
    pub fn new() -> Self {
        let mut tree_sitter_parser = Parser::new();
        tree_sitter_parser.set_language(tree_sitter_glsl::language()).unwrap();
        ServerData {
            extensions: RefCell::new(HashSet::new()),
            shader_packs: RefCell::new(HashSet::new()),
            workspace_files: RefCell::new(HashMap::new()),
            temp_files: RefCell::new(HashMap::new()),
            tree_sitter_parser: RefCell::new(tree_sitter_parser),
        }
    }

    pub fn workspace_files(&self) -> &RefCell<HashMap<PathBuf, WorkspaceFile>> {
        &self.workspace_files
    }

    pub fn temp_files(&self) -> &RefCell<HashMap<PathBuf, TempFile>> {
        &self.temp_files
    }
}
