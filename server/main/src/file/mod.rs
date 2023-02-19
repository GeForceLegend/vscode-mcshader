use std::{
    collections::HashSet,
    path::PathBuf,
};

mod include_file;
mod shader_file;
mod temp_file;

#[derive(Clone)]
pub struct ShaderFile {
    /// Live content for this file
    content: String,
    /// Type of the shader
    file_type: gl::types::GLenum,
    /// The shader pack path that this file in
    pack_path: PathBuf,
}

#[derive(Clone)]
pub struct IncludeFile {
    /// Live content for this file
    content: String,
    /// The shader pack path that this file in
    pack_path: PathBuf,
    /// Shader files that include this file
    included_shaders: HashSet<PathBuf>,
    /// Files included in this file
    /// Though we can scan its content and get includes,
    /// keep a collection helps update parents faster
    including_files: HashSet<PathBuf>,
}

#[derive(Clone)]
pub struct TempFile {
    /// Live content for this file
    content: String,
    /// Type of the shader
    file_type: gl::types::GLenum,
    /// The shader pack path that this file in
    pack_path: PathBuf,
}
