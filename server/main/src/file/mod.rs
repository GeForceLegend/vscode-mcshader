use std::{
    collections::HashSet,
    path::PathBuf,
};

mod include_file;
mod shader_file;
mod temp_file;

#[derive(Clone)]
pub struct ShaderFile {
    /// File path
    file_path: PathBuf,
    /// Live content for this file
    content: String,
    /// Type of the shader
    file_type: gl::types::GLenum,
    /// The shader pack path that this file in
    pack_path: PathBuf,
}

#[derive(Clone)]
pub struct IncludeFile {
    /// File path
    file_path: PathBuf,
    /// Live content for this file
    content: String,
    /// The shader pack path that this file in
    pack_path: PathBuf,
    /// Shader files that include this file
    included_shaders: HashSet<PathBuf>,
    /// Files included in this file (line, start char, end char, file path)
    including_files: HashSet<PathBuf>,
}

#[derive(Clone)]
pub struct TempFile {
    /// File path
    file_path: PathBuf,
    /// Live content for this file
    content: String,
    /// Type of the shader
    file_type: gl::types::GLenum,
    /// The shader pack path that this file in
    pack_path: PathBuf,
}
