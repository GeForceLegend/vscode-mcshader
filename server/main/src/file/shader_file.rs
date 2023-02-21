
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::MutexGuard,
    fs::read_to_string,
};

use logging::error;
use path_slash::PathBufExt;

use crate::constant::{
    RE_MACRO_INCLUDE,
    RE_MACRO_LINE,
    RE_MACRO_VERSION,
    OPTIFINE_MACROS,
};

use super::{ShaderFile, IncludeFile};

impl ShaderFile {
    pub fn content(&self) -> &String {
        &self.content
    }

    pub fn content_mut(&mut self) -> &mut String {
        &mut self.content
    }

    pub fn file_type(&self) -> gl::types::GLenum {
        self.file_type
    }

    pub fn pack_path(&self) -> &PathBuf {
        &self.pack_path
    }

    /// Create a new shader file, load contents from given path, and add includes to the list
    pub fn new(pack_path: &PathBuf, file_path: &PathBuf, include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>) -> ShaderFile {
        let extension = file_path.extension().unwrap();
        let mut shader_file = ShaderFile {
            content: String::new(),
            file_type: {
                if extension == "fsh" {
                    gl::FRAGMENT_SHADER
                } else if extension == "vsh" {
                    gl::VERTEX_SHADER
                } else if extension == "gsh" {
                    gl::GEOMETRY_SHADER
                } else if extension == "csh" {
                    gl::COMPUTE_SHADER
                } else {
                    gl::NONE
                }
            },
            pack_path: pack_path.clone(),
        };
        shader_file.update_shader(include_files, file_path);
        shader_file
    }

    /// Update shader content and includes from file
    pub fn update_shader (&mut self, include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, file_path: &PathBuf) {
        if let Ok(content) =  read_to_string(file_path) {
            let parent_path: HashSet<PathBuf> = HashSet::from([file_path.clone()]);
            let mut parent_update_list: HashSet<PathBuf> = HashSet::new();
            content.lines()
                .for_each(|line| {
                    if let Some(capture) = RE_MACRO_INCLUDE.captures(line) {
                        let path = capture.get(1).unwrap().as_str();

                        let include_path = match path.strip_prefix('/') {
                            Some(path) => self.pack_path.join(PathBuf::from_slash(path)),
                            None => file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                        };

                        IncludeFile::get_includes(include_files, &mut parent_update_list, &self.pack_path, include_path, &parent_path, 0);
                    }
                });
            for include_file in parent_update_list {
                include_files.get_mut(&include_file).unwrap().included_shaders.insert(file_path.clone());
            }
            self.content = content;
        }
        else {
            error!("Unable to read file {}", file_path.display());
        }
    }

    /// Merge all includes to one vitrual file for compiling etc
    pub fn merge_shader_file(&self, include_files: &MutexGuard<HashMap<PathBuf, IncludeFile>>,
        file_path: &PathBuf, file_list: &mut HashMap<String, PathBuf>
    ) -> String {
        let mut shader_content: String = String::new();
        file_list.insert("0".to_owned(), file_path.clone());
        let mut file_id = 0;
        let file_name = file_path.display();

        // If we are in the debug folder, do not add Optifine's macros
        let mut macro_insert = self.pack_path.parent().unwrap().file_name().unwrap() != "debug";

        self.content.lines()
            .enumerate()
            .for_each(|line| {
                if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                    let path: String = capture.get(1).unwrap().as_str().into();

                    let include_path = match path.strip_prefix('/') {
                        Some(path) => self.pack_path.join(PathBuf::from_slash(path)),
                        None => file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                    };

                    if let Some(include_file) = include_files.get(&include_path) {
                        let include_content = include_file.merge_include(include_files, include_path, line.1.to_string(), file_list, &mut file_id, 1);
                        shader_content += &include_content;
                        shader_content += &format!("#line {} 0\t//{}\n", line.0 + 2, file_name);
                    }
                    else {
                        shader_content += line.1;
                        shader_content += "\n";
                    }
                }
                else if RE_MACRO_LINE.is_match(line.1) {
                    // Delete existing #line for correct linting
                    shader_content += "\n";
                }
                else {
                    shader_content += line.1;
                    shader_content += "\n";
                    // If we are not in the debug folder, add Optifine's macros for correct linting
                    if macro_insert && RE_MACRO_VERSION.is_match(line.1) {
                        shader_content += OPTIFINE_MACROS;
                        shader_content += &format!("#line {} 0\n", line.0 + 2);
                        macro_insert = false;
                    }
                }
            });

        shader_content
    }
}
