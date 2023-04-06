use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs::read_to_string,
    path::PathBuf,
};

use logging::error;
use tree_sitter::{Parser, Tree};

use super::*;

impl ShaderFile {
    /// Create a new shader file, load contents from given path, and add includes to the list
    pub fn new(
        include_files: &mut HashMap<PathBuf, IncludeFile>, parser: &mut Parser, pack_path: &PathBuf, file_path: &PathBuf,
    ) -> ShaderFile {
        let extension = file_path.extension().unwrap();
        let shader_file = ShaderFile {
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
            content: RefCell::from(String::new()),
            tree: RefCell::from(parser.parse("", None).unwrap()),
        };
        shader_file.update_shader(include_files, parser, file_path);
        shader_file
    }

    /// Update shader content and includes from file
    pub fn update_shader(&self, include_files: &mut HashMap<PathBuf, IncludeFile>, parser: &mut Parser, file_path: &PathBuf) {
        if let Ok(content) = read_to_string(file_path) {
            let parent_path: HashSet<PathBuf> = HashSet::from([file_path.clone()]);
            let mut parent_update_list: HashSet<PathBuf> = HashSet::new();
            content.lines().for_each(|line| {
                if let Some(capture) = RE_MACRO_INCLUDE.captures(line) {
                    let path = capture.get(1).unwrap().as_str();

                    match include_path_join(&self.pack_path, file_path, path) {
                        Ok(include_path) => IncludeFile::get_includes(
                            include_files,
                            &mut parent_update_list,
                            parser,
                            &self.pack_path,
                            include_path,
                            &parent_path,
                            0,
                        ),
                        Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
                    }
                }
            });
            for include_file in parent_update_list {
                include_files
                    .get_mut(&include_file)
                    .unwrap()
                    .included_shaders
                    .borrow_mut()
                    .insert(file_path.clone());
            }
            *self.tree.borrow_mut() = parser.parse(&content, None).unwrap();
            *self.content.borrow_mut() = content;
        } else {
            error!("Unable to read file {}", file_path.display());
        }
    }

    /// Merge all includes to one vitrual file for compiling etc
    pub fn merge_shader_file(
        &self, include_files: &HashMap<PathBuf, IncludeFile>, file_path: &PathBuf, file_list: &mut HashMap<String, PathBuf>,
    ) -> String {
        let mut shader_content: Vec<u8> = Vec::new();
        file_list.insert(String::from("0"), file_path.clone());
        let mut file_id = 0;
        let file_name = file_path.display().to_string();

        self.content.borrow().lines().enumerate().for_each(|line| {
            if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                let path = capture.get(1).unwrap().as_str();

                if let Ok(include_path) = include_path_join(&self.pack_path, file_path, path) {
                    if let Some(include_file) = include_files.get(&include_path) {
                        let include_content = include_file.merge_include(include_files, include_path, line.1, file_list, &mut file_id, 1);
                        shader_content.extend(include_content);
                        shader_content.extend(format!("#line {} 0\t//{}\n", line.0 + 2, file_name).into_bytes());
                    } else {
                        shader_content.extend(line.1.as_bytes());
                        shader_content.push(b'\n');
                    }
                } else {
                    shader_content.extend(line.1.as_bytes());
                    shader_content.push(b'\n');
                }
            } else if RE_MACRO_LINE.is_match(line.1) {
                // Delete existing #line for correct linting
                shader_content.push(b'\n');
            } else {
                shader_content.extend(line.1.as_bytes());
                shader_content.push(b'\n');
            }
        });

        let mut shader_content = unsafe { String::from_utf8_unchecked(shader_content) };

        // Move #version to the top line
        if let Some(capture) = RE_MACRO_VERSION.captures(&shader_content) {
            let version = capture.get(0).unwrap();
            let mut version_content = version.as_str().to_owned() + "\n";

            shader_content.replace_range(version.start()..version.end(), "");
            // If we are not in the debug folder, add Optifine's macros
            if self.pack_path.parent().unwrap().file_name().unwrap() != "debug" {
                version_content += OPTIFINE_MACROS;
            }
            version_content += &format!("#line 1 0\t//{}\n", file_name);
            shader_content.insert_str(0, &version_content);
        }

        shader_content
    }
}

impl File for ShaderFile {
    fn pack_path(&self) -> &PathBuf {
        &self.pack_path
    }

    fn content(&self) -> &RefCell<String> {
        &self.content
    }

    fn tree(&self) -> &RefCell<Tree> {
        &self.tree
    }
}

impl BaseShader for ShaderFile {
    fn file_type(&self) -> gl::types::GLenum {
        self.file_type
    }
}
