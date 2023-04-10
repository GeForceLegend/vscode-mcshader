use std::{cell::RefCell, collections::HashMap, fs::read_to_string, path::PathBuf};

use logging::warn;
use tree_sitter::{Parser, Tree};

use super::*;

impl TempFile {
    pub fn new(parser: &mut Parser, file_path: &PathBuf) -> Option<Self> {
        warn!("Document not found in file system"; "path" => file_path.display());
        let content = match read_to_string(file_path) {
            Ok(content) => RefCell::from(content),
            Err(_err) => RefCell::from(String::new()),
        };
        let file_type = match file_path.extension() {
            Some(extension) => {
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
            }
            None => gl::NONE,
        };
        let mut pack_path = file_path.clone();
        loop {
            if !pack_path.pop() {
                return None;
            }
            match pack_path.file_name() {
                Some(file_name) if file_name == "shaders" => break,
                Some(_) => continue,
                None => return None,
            }
        }
        Some(TempFile {
            content,
            file_type,
            pack_path,
            tree: RefCell::from(parser.parse("", None).unwrap()),
        })
    }

    pub fn update_self(&mut self, file_path: &PathBuf) {
        *self.content.borrow_mut() = match read_to_string(file_path) {
            Ok(content) => content,
            Err(_err) => String::new(),
        };
    }

    pub fn merge_self(&self, file_path: &PathBuf, file_list: &mut HashMap<String, PathBuf>) -> Option<(gl::types::GLenum, String)> {
        if self.file_type == gl::NONE {
            return None;
        }

        let mut temp_content = String::new();
        file_list.insert("0".to_owned(), file_path.clone());
        let mut file_id = 0;
        let file_name = file_path.to_str().unwrap();

        let content = self.content.borrow();
        let mut start_index = 0;
        let mut lines = 2;

        RE_MACRO_CATCH.captures_iter(content.as_ref()).for_each(|captures| {
            let capture = captures.get(0).unwrap();
            let start = capture.start();
            let end = capture.end();

            let before_content = unsafe { content.get_unchecked(start_index..start) };
            let capture_content = capture.as_str();
            if let Some(capture) = RE_MACRO_INCLUDE.captures(capture_content) {
                let path = capture.get(1).unwrap().as_str();

                let include_path = match path.strip_prefix('/') {
                    Some(path) => self.pack_path.join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                    None => file_path
                        .parent()
                        .unwrap()
                        .join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                };
                temp_content += before_content;
                start_index = end;
                lines += before_content.matches("\n").count();

                let include_content = Self::merge_temp(&self.pack_path, include_path, file_list, capture_content, &mut file_id, 1);
                temp_content += &include_content;
                temp_content += "\n";
                temp_content += &generate_line_macro(lines, "0", file_name);
            } else if RE_MACRO_LINE.is_match(capture_content) {
                temp_content += before_content;
                start_index = end;
                lines += before_content.matches("\n").count();

                temp_content += "\n";
            }
        });
        temp_content += unsafe { content.get_unchecked(start_index..) };

        // Move #version to the top line
        if let Some(capture) = RE_MACRO_VERSION.captures(&temp_content) {
            let version = capture.get(0).unwrap();
            let mut version_content = version.as_str().to_owned() + "\n";

            temp_content.replace_range(version.start()..version.end(), "");
            // If we are not in the debug folder, add Optifine's macros
            if self.pack_path.parent().unwrap().file_name().unwrap() != "debug" {
                version_content += OPTIFINE_MACROS;
            }
            version_content += "#line 1 0\t//";
            version_content += file_name;
            version_content += "\n";
            temp_content.insert_str(0, &version_content);
        }

        Some((self.file_type, temp_content))
    }

    fn merge_temp(
        pack_path: &PathBuf, file_path: PathBuf, file_list: &mut HashMap<String, PathBuf>, original_content: &str, file_id: &mut i32,
        depth: i32,
    ) -> String {
        if depth > 10 || !file_path.exists() {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return original_content.to_owned() + "\n";
        }
        *file_id += 1;
        let curr_file_id = Buffer::new().format(*file_id).to_owned();
        let file_name = file_path.to_str().unwrap();
        let mut include_content = generate_line_macro(1, &curr_file_id, file_name) + "\n";

        if let Ok(content) = read_to_string(&file_path) {
            let mut start_index = 0;
            let mut lines = 2;

            RE_MACRO_CATCH.captures_iter(content.as_ref()).for_each(|captures| {
                let capture = captures.get(0).unwrap();
                let start = capture.start();
                let end = capture.end();

                let before_content = unsafe { content.get_unchecked(start_index..start) };
                let capture_content = capture.as_str();
                if let Some(capture) = RE_MACRO_INCLUDE.captures(capture_content) {
                    let path = capture.get(1).unwrap().as_str();

                    let include_path = match path.strip_prefix('/') {
                        Some(path) => pack_path.join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                        None => file_path
                            .parent()
                            .unwrap()
                            .join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                    };
                    include_content += before_content;
                    start_index = end;
                    lines += before_content.matches("\n").count();

                    let sub_include_content = Self::merge_temp(pack_path, include_path, file_list, capture_content, file_id, depth + 1);
                    include_content += &sub_include_content;
                    include_content += "\n";
                    include_content += &generate_line_macro(lines, &curr_file_id, file_name);
                } else if !RE_MACRO_LINE.is_match(capture_content) {
                    include_content += before_content;
                    start_index = end;
                    lines += before_content.matches("\n").count();
                    include_content += capture_content;
                }
            });
            include_content += unsafe { content.get_unchecked(start_index..) };
            file_list.insert(curr_file_id, file_path);
            include_content
        } else {
            warn!("Unable to read file"; "path" => file_path.display());
            original_content.to_owned() + "\n"
        }
    }
}

impl File for TempFile {
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
