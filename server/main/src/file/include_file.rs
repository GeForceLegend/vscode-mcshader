use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs::read_to_string,
    path::PathBuf,
};

use logging::error;
use tree_sitter::{Parser, Tree};

use super::*;

impl IncludeFile {
    pub fn included_shaders(&self) -> &RefCell<HashSet<PathBuf>> {
        &self.included_shaders
    }

    pub fn including_files(&self) -> &RefCell<HashSet<PathBuf>> {
        &self.including_files
    }

    pub fn parent_update_list(&self, include_files: &HashMap<PathBuf, IncludeFile>, update_list: &mut HashSet<PathBuf>, depth: i32) {
        if depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return;
        }
        // Insert files that need to update parents into a list
        for file in self.including_files.borrow().iter() {
            if let Some(include_file) = include_files.get(file) {
                update_list.insert(file.clone());
                include_file.parent_update_list(include_files, update_list, depth + 1);
            }
        }
    }

    pub fn get_includes(
        include_files: &mut HashMap<PathBuf, IncludeFile>, parent_update_list: &mut HashSet<PathBuf>, parser: &mut Parser,
        pack_path: &PathBuf, include_path: PathBuf, parent_file: &HashSet<PathBuf>, depth: i32,
    ) {
        if !include_path.exists() || depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return;
        } else if let Some(include_file) = include_files.get(&include_path) {
            // Insert all include files that need to update parent shader to a list
            // And add parent shader together
            parent_update_list.insert(include_path);
            include_file.parent_update_list(include_files, parent_update_list, depth + 1);
        } else {
            if let Ok(content) = read_to_string(&include_path) {
                let mut including_files = HashSet::new();
                content.lines().for_each(|line| {
                    if let Some(capture) = RE_MACRO_INCLUDE.captures(line) {
                        let path = capture.get(1).unwrap().as_str();

                        match include_path_join(pack_path, &include_path, path) {
                            Ok(sub_include_path) => {
                                including_files.insert(sub_include_path.clone());
                                Self::get_includes(
                                    include_files,
                                    parent_update_list,
                                    parser,
                                    pack_path,
                                    sub_include_path,
                                    parent_file,
                                    depth + 1,
                                );
                            }
                            Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
                        }
                    }
                });
                let include_file = IncludeFile {
                    tree: RefCell::from(parser.parse(&content, None).unwrap()),
                    content: RefCell::from(content),
                    pack_path: pack_path.clone(),
                    included_shaders: RefCell::from(parent_file.clone()),
                    including_files: RefCell::from(including_files),
                };
                include_files.insert(include_path, include_file);
            } else {
                error!("Unable to read file {}", include_path.display());
            }
        }
    }

    pub fn update_include(&self, include_files: &mut HashMap<PathBuf, IncludeFile>, parser: &mut Parser, file_path: &PathBuf) {
        let mut including_files = self.including_files.borrow_mut();
        including_files.clear();

        if let Ok(content) = read_to_string(file_path) {
            let mut parent_update_list: HashSet<PathBuf> = HashSet::new();
            let included_shaders = self.included_shaders.borrow();
            content.lines().for_each(|line| {
                if let Some(capture) = RE_MACRO_INCLUDE.captures(line) {
                    let path = capture.get(1).unwrap().as_str();

                    match include_path_join(&self.pack_path, file_path, path) {
                        Ok(sub_include_path) => {
                            including_files.insert(sub_include_path.clone());
                            Self::get_includes(
                                include_files,
                                &mut parent_update_list,
                                parser,
                                &self.pack_path,
                                sub_include_path,
                                &included_shaders,
                                1,
                            );
                        }
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
                    .extend(included_shaders.clone());
            }
            *self.content.borrow_mut() = content;
        } else {
            error!("Unable to read file"; "path" => file_path.display());
        }
    }

    pub fn merge_include(
        &self, include_files: &HashMap<PathBuf, IncludeFile>, file_path: PathBuf, original_content: &str,
        file_list: &mut HashMap<String, PathBuf>, file_id: &mut i32, depth: i32,
    ) -> Vec<u8> {
        if !file_path.exists() || depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            let mut original_content_vec: Vec<u8> = Vec::with_capacity(original_content.len() + 1);
            original_content_vec.extend(original_content.as_bytes());
            original_content_vec.push(b'\n');
            return original_content_vec;
        }
        *file_id += 1;
        let curr_file_id = file_id.to_string();
        let file_name = file_path.display().to_string();
        let mut include_content = format!("#line 1 {}\t//{}\n", curr_file_id, file_name).into_bytes();

        self.content.borrow().lines().enumerate().for_each(|line| {
            if RE_MACRO_CATCH.is_match(line.1) {
                if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                    let path = capture.get(1).unwrap().as_str();

                    if let Ok(include_path) = include_path_join(&self.pack_path, &file_path, path) {
                        if let Some(include_file) = include_files.get(&include_path) {
                            let sub_include_content =
                                include_file.merge_include(include_files, include_path, line.1, file_list, file_id, depth + 1);
                            include_content.extend(sub_include_content);
                            include_content.extend(format!("#line {} {}\t//{}\n", line.0 + 2, curr_file_id, file_name).into_bytes());
                        } else {
                            include_content.extend(line.1.as_bytes());
                            include_content.push(b'\n');
                        }
                    } else {
                        include_content.extend(line.1.as_bytes());
                        include_content.push(b'\n');
                    }
                } else if RE_MACRO_LINE.is_match(line.1) {
                    // Delete existing #line for correct linting
                    include_content.push(b'\n');
                } else {
                    include_content.extend(line.1.as_bytes());
                    include_content.push(b'\n');
                }
            } else {
                include_content.extend(line.1.as_bytes());
                include_content.push(b'\n');
            }
        });
        file_list.insert(curr_file_id, file_path);
        include_content
    }
}

impl File for IncludeFile {
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
