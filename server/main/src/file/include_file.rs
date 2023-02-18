use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::MutexGuard,
    fs::read_to_string,
};

use logging::warn;
use path_slash::PathBufExt;

use slog_scope::error;

use crate::constant::{
    RE_MACRO_INCLUDE,
    RE_MACRO_LINE,
};

use super::IncludeFile;

impl IncludeFile {
    pub fn content(&self) -> &String {
        &self.content
    }

    pub fn content_mut(&mut self) -> &mut String {
        &mut self.content
    }

    pub fn pack_path(&self) -> &PathBuf {
        &self.pack_path
    }

    pub fn included_shaders(&self) -> &HashSet<PathBuf> {
        &self.included_shaders
    }

    pub fn included_shaders_mut(&mut self) -> &mut HashSet<PathBuf> {
        &mut self.included_shaders
    }

    pub fn update_parent(include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, include_path: &PathBuf, parent_file: &HashSet<PathBuf>, depth: i32) {
        if depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return;
        }
        let include_file = include_files.get_mut(include_path).unwrap();
        include_file.included_shaders.extend(parent_file.clone());

        for include_path in include_file.including_files.clone() {
            Self::update_parent(include_files, &include_path, parent_file, depth + 1);
        }
    }

    pub fn get_includes(include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>,
        pack_path: &PathBuf, include_path: PathBuf, parent_file: &HashSet<PathBuf>, depth: i32
    ) {
        if !include_path.exists() || depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return;
        }
        else if include_files.contains_key(&include_path) {
            Self::update_parent(include_files, &include_path, parent_file, depth);
        }
        else {
            let mut include_file = IncludeFile {
                content: String::new(),
                pack_path: pack_path.clone(),
                included_shaders: parent_file.clone(),
                including_files: HashSet::new(),
            };

            if let Ok(content) = read_to_string(&include_path) {
                content.lines()
                    .for_each(|line| {
                        if let Some(capture) = RE_MACRO_INCLUDE.captures(line) {
                            let path: String = capture.get(1).unwrap().as_str().into();

                            let sub_include_path = match path.strip_prefix('/') {
                                Some(path) => pack_path.join(PathBuf::from_slash(path)),
                                None => include_path.parent().unwrap().join(PathBuf::from_slash(&path))
                            };

                            include_file.including_files.insert(sub_include_path.clone());

                            Self::get_includes(include_files, pack_path, sub_include_path, parent_file, depth + 1);
                        }
                    });
                include_file.content = content;
            }
            else {
                error!("Unable to read file {}", include_path.to_str().unwrap());
            }

            include_files.insert(include_path, include_file);
        }
    }

    pub fn update_include(&mut self, include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, file_path: &PathBuf) {
        self.including_files.clear();

        if let Ok(content) = read_to_string(file_path) {
            content.lines()
                .for_each(|line| {
                    if let Some(capture) = RE_MACRO_INCLUDE.captures(line) {
                        let path: String = capture.get(1).unwrap().as_str().into();

                        let sub_include_path = match path.strip_prefix('/') {
                            Some(path) => self.pack_path.join(PathBuf::from_slash(path)),
                            None => file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                        };

                        self.including_files.insert(sub_include_path.clone());

                        Self::get_includes(include_files, &self.pack_path, sub_include_path, &self.included_shaders, 1);
                    }
                });
            self.content = content;
        }
        else {
            warn!("Unable to read file"; "path" => file_path.to_str().unwrap());
        }
    }

    pub fn merge_include(&self, include_files: &MutexGuard<HashMap<PathBuf, IncludeFile>>, file_path: &PathBuf,
        original_content: String, file_list: &mut HashMap<String, PathBuf>, file_id: &mut i32, depth: i32
    ) -> String {
        if !file_path.exists() || depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            original_content + "\n"
        }
        else {
            let mut include_content: String = String::new();
            file_list.insert(file_id.to_string(), file_path.clone());
            include_content += &format!("#line 1 {}\n", &file_id.to_string());
            let curr_file_id = file_id.clone();

            self.content.lines()
                .enumerate()
                .for_each(|line| {
                    if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                        *file_id += 1;
                        let path: String = capture.get(1).unwrap().as_str().into();

                        let include_path = match path.strip_prefix('/') {
                            Some(path) => self.pack_path.join(PathBuf::from_slash(path)),
                            None => file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                        };

                        if let Some(include_file) = include_files.get(&include_path) {
                            let sub_include_content = include_file.merge_include(include_files, &include_path, line.1.to_string(), file_list, file_id, 1);
                            include_content += &sub_include_content;
                            include_content += &format!("#line {} {}\n", line.0 + 2, curr_file_id);
                        }
                        else {
                            include_content += line.1;
                            include_content += "\n";
                        }
                    }
                    else if RE_MACRO_LINE.is_match(line.1) {
                        // Delete existing #line for correct linting
                        include_content += "\n";
                    }
                    else {
                        include_content += line.1;
                        include_content += "\n";
                    }
                });
            include_content
        }
    }
}
