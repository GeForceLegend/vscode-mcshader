use std::{
    cell::RefCell,
    collections::{HashSet, HashMap},
    path::PathBuf,
};

use tree_sitter::Parser;

use crate::file::{ShaderFile, IncludeFile, TempFile};

use super::ServerData;

impl ServerData {
    pub fn new(parser: Parser) -> Self {
        ServerData {
            extensions: RefCell::from(HashSet::new()),
            roots: RefCell::from(HashSet::new()),
            shader_packs: RefCell::from(HashSet::new()),
            shader_files: RefCell::from(HashMap::new()),
            include_files: RefCell::from(HashMap::new()),
            temp_files: RefCell::from(HashMap::new()),
            tree_sitter_parser: RefCell::from(parser),
        }
    }

    pub fn shader_files(&self) -> &RefCell<HashMap<PathBuf, ShaderFile>>{
        &self.shader_files
    }

    pub fn include_files(&self) -> &RefCell<HashMap<PathBuf, IncludeFile>>{
        &self.include_files
    }

    pub fn temp_files(&self) -> &RefCell<HashMap<PathBuf, TempFile>>{
        &self.temp_files
    }
}
