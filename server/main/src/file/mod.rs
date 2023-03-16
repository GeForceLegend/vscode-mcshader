use std::{
    collections::HashSet,
    path::PathBuf,
    cell::RefCell,
};

use tree_sitter::Tree;

mod include_file;
mod shader_file;
mod temp_file;

#[derive(Clone)]
pub struct ShaderFile {
    /// Type of the shader
    file_type: gl::types::GLenum,
    /// The shader pack path that this file in
    pack_path: PathBuf,
    /// Live content for this file
    content: RefCell<String>,
    /// Live syntax tree for this file
    tree: RefCell<Tree>,
}

#[derive(Clone)]
pub struct IncludeFile {
    /// The shader pack path that this file in
    pack_path: PathBuf,
    /// Live content for this file
    content: RefCell<String>,
    /// Live syntax tree for this file
    tree: RefCell<Tree>,
    /// Shader files that include this file
    included_shaders: RefCell<HashSet<PathBuf>>,
    /// Files included in this file
    /// Though we can scan its content and get includes,
    /// keep a collection helps update parents faster
    including_files: RefCell<HashSet<PathBuf>>,
}

#[derive(Clone)]
pub struct TempFile {
    /// Type of the shader
    file_type: gl::types::GLenum,
    /// The shader pack path that this file in
    pack_path: PathBuf,
    /// Live content for this file
    content: RefCell<String>,
    /// Live syntax tree for this file
    tree: RefCell<Tree>,
}
