use std::{cell::RefCell, collections::HashMap, fs::read_to_string, path::PathBuf};

use logging::warn;
use tree_sitter::{Parser, Tree};

use crate::constant::{OPTIFINE_MACROS, RE_MACRO_INCLUDE, RE_MACRO_LINE, RE_MACRO_VERSION};

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
        file_list.insert(String::from("0"), file_path.clone());
        let mut file_id = 0;
        let file_name = file_path.display();

        // If we are in the debug folder, do not add Optifine's macros
        let mut macro_inserted = self.pack_path.parent().unwrap().file_name().unwrap() == "debug";

        self.content.borrow().lines().enumerate().for_each(|line| {
            if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                let path = capture.get(1).unwrap().as_str();

                let include_path = match path.strip_prefix('/') {
                    Some(path) => self.pack_path.join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                    None => file_path
                        .parent()
                        .unwrap()
                        .join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                };

                let include_content = Self::merge_temp(&self.pack_path, include_path, file_list, String::from(line.1), &mut file_id, 1);
                temp_content += &include_content;
                temp_content += &format!("#line {} 0\t//{}\n", line.0 + 2, file_name);
            } else if RE_MACRO_LINE.is_match(line.1) {
                // Delete existing #line for correct linting
                temp_content += "\n";
            } else {
                temp_content += line.1;
                temp_content += "\n";
                // If we are not in the debug folder, add Optifine's macros for correct linting
                if !macro_inserted && RE_MACRO_VERSION.is_match(line.1) {
                    temp_content += OPTIFINE_MACROS;
                    temp_content += &format!("#line {} 0\t//{}\n", line.0 + 2, file_name);
                    macro_inserted = true;
                }
            }
        });

        Some((self.file_type, temp_content))
    }

    fn merge_temp(
        pack_path: &PathBuf, file_path: PathBuf, file_list: &mut HashMap<String, PathBuf>, original_content: String, file_id: &mut i32,
        depth: i32,
    ) -> String {
        if depth > 10 || !file_path.exists() {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return original_content + "\n";
        }
        *file_id += 1;
        let curr_file_id = file_id.to_string();
        let file_name = file_path.display();
        let mut include_content = format!("#line 1 {}\t//{}\n", curr_file_id, file_name);

        if let Ok(content) = read_to_string(&file_path) {
            content.lines().enumerate().for_each(|line| {
                if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                    let path = capture.get(1).unwrap().as_str();

                    let include_path = match path.strip_prefix('/') {
                        Some(path) => pack_path.join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                        None => file_path
                            .parent()
                            .unwrap()
                            .join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                    };

                    let sub_include_content =
                        Self::merge_temp(pack_path, include_path, file_list, String::from(line.1), file_id, depth + 1);
                    include_content += &sub_include_content;

                    include_content += &format!("#line {} {}\t//{}\n", line.0 + 2, curr_file_id, file_name);
                } else if RE_MACRO_LINE.is_match(&line.1) {
                    // Delete existing #line for correct linting
                    include_content += "\n";
                } else {
                    include_content += &line.1;
                    include_content += "\n";
                }
            });
            file_list.insert(curr_file_id, file_path);
            include_content
        } else {
            warn!("Unable to read file"; "path" => file_path.display());
            original_content + "\n"
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
