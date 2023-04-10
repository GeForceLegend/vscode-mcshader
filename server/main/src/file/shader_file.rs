use std::{cell::RefCell, fs::read_to_string, path::PathBuf};

use hashbrown::{HashMap, HashSet};
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
            RE_MACRO_INCLUDE_MULTI_LINE.captures_iter(&content).for_each(|captures| {
                let path = captures.get(1).unwrap().as_str();

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
            });
            for include_path in parent_update_list {
                include_files
                    .get_mut(&include_path)
                    .unwrap()
                    .included_shaders
                    .borrow_mut()
                    .insert(file_path.clone());
            }
            *self.tree.borrow_mut() = parser.parse(&content, None).unwrap();
            *self.content.borrow_mut() = content;
        } else {
            error!("Unable to read file {}", file_path.to_str().unwrap());
        }
    }

    /// Merge all includes to one vitrual file for compiling etc
    pub fn merge_shader_file(
        &self, include_files: &HashMap<PathBuf, IncludeFile>, file_path: &PathBuf, file_list: &mut HashMap<String, PathBuf>,
    ) -> String {
        let mut shader_content = String::new();
        file_list.insert("0".to_owned(), file_path.clone());
        let mut file_id = 0;
        let file_name = file_path.to_str().unwrap();

        let content = self.content.borrow();
        let mut start_index = 0;
        let mut lines = 2;

        RE_MACRO_CATCH.find_iter(content.as_ref()).for_each(|macro_line| {
            let start = macro_line.start();
            let end = macro_line.end();

            let before_content = unsafe { content.get_unchecked(start_index..start) };
            let capture_content = macro_line.as_str();
            if let Some(capture) = RE_MACRO_INCLUDE.captures(capture_content) {
                let path = capture.get(1).unwrap().as_str();

                if let Ok(include_path) = include_path_join(&self.pack_path, file_path, path) {
                    if let Some(include_file) = include_files.get(&include_path) {
                        shader_content += before_content;
                        start_index = end;
                        lines += before_content.matches("\n").count();

                        let include_content =
                            include_file.merge_include(include_files, include_path, capture_content, file_list, &mut file_id, 1);
                        shader_content += &include_content;
                        shader_content += "\n";
                        shader_content += &generate_line_macro(lines, "0", file_name);
                    }
                }
            } else if RE_MACRO_LINE.is_match(capture_content) {
                shader_content += before_content;
                start_index = end;
                lines += before_content.matches("\n").count();

                shader_content += capture_content;
            }
        });
        shader_content += unsafe { content.get_unchecked(start_index..) };

        // Move #version to the top line
        if let Some(capture) = RE_MACRO_VERSION.captures(&shader_content) {
            let version = capture.get(0).unwrap();
            let mut version_content = version.as_str().to_owned() + "\n";

            shader_content.replace_range(version.start()..version.end(), "");
            // If we are not in the debug folder, add Optifine's macros
            if self.pack_path.parent().unwrap().file_name().unwrap() != "debug" {
                version_content += OPTIFINE_MACROS;
            }
            version_content += "#line 1 0\t//";
            version_content += file_name;
            version_content += "\n";
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
    fn file_type(&self) -> u32 {
        self.file_type
    }
}
