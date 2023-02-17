use std::{
    collections::{HashMap, HashSet, LinkedList},
    path::{PathBuf},
    io::{BufReader, BufRead},
    sync::MutexGuard,
    fs::read_to_string,
};

use logging::warn;
use path_slash::PathBufExt;

use slog_scope::error;
use tower_lsp::lsp_types::*;

use crate::constant::{
    RE_MACRO_INCLUDE,
    RE_MACRO_LINE,
    RE_MACRO_VERSION,
    OPTIFINE_MACROS,
};

pub fn parse_includes(content: &String, pack_path: &PathBuf, file_path: &PathBuf) -> Vec<DocumentLink> {
    let mut include_links = Vec::new();

    content.lines()
        .enumerate()
        .filter(|line| RE_MACRO_INCLUDE.is_match(line.1))
        .for_each(|line| {
            let cap = RE_MACRO_INCLUDE.captures(line.1).unwrap().get(1).unwrap();
            let path: String = cap.as_str().into();

            let start = cap.start();
            let end = cap.end();

            let include_path = if path.starts_with('/') {
                let path = path.strip_prefix('/').unwrap().to_string();
                pack_path.join(PathBuf::from_slash(&path))
            } else {
                file_path.parent().unwrap().join(PathBuf::from_slash(&path))
            };
            let url = Url::from_file_path(include_path).unwrap();

            include_links.push(DocumentLink {
                range: Range::new(
                    Position::new(u32::try_from(line.0).unwrap(), u32::try_from(start).unwrap()),
                    Position::new(u32::try_from(line.0).unwrap(), u32::try_from(end).unwrap()),
                ),
                tooltip: Some(url.path().to_string()),
                target: Some(url),
                data: None,
            });
        });
    include_links
}

#[derive(Clone)]
pub struct ShaderFile {
    // File path
    file_path: PathBuf,
    // Live content for this file
    content: String,
    // Type of the shader
    file_type: gl::types::GLenum,
    // The shader pack path that this file in
    pack_path: PathBuf,
    // Files included in this file (line, start char, end char, file path)
    including_files: LinkedList<(usize, usize, usize, PathBuf)>,
}

impl ShaderFile {
    pub fn content(&self) -> &String {
        &self.content
    }

    pub fn content_mut(&mut self) -> &mut String {
        &mut self.content
    }

    pub fn file_type(&self) -> &gl::types::GLenum {
        &self.file_type
    }

    pub fn pack_path(&self) -> &PathBuf {
        &self.pack_path
    }

    pub fn clear_including_files(&mut self) {
        self.including_files.clear();
    }

    pub fn new(pack_path: &PathBuf, file_path: &PathBuf) -> ShaderFile {
        ShaderFile {
            file_path: file_path.clone(),
            content: String::new(),
            file_type: gl::NONE,
            pack_path: pack_path.clone(),
            including_files: LinkedList::new(),
        }
    }

    pub fn read_file (&mut self, include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>) {
        let extension = self.file_path.extension().unwrap();
        self.file_type = if extension == "fsh" {
                gl::FRAGMENT_SHADER
            } else if extension == "vsh" {
                gl::VERTEX_SHADER
            } else if extension == "gsh" {
                gl::GEOMETRY_SHADER
            } else if extension == "csh" {
                gl::COMPUTE_SHADER
            } else {
                gl::NONE
            };


        match read_to_string(&self.file_path) {
            Ok(content) => {
                content.lines()
                    .enumerate()
                    .filter(|line| RE_MACRO_INCLUDE.is_match(line.1))
                    .for_each(|line| {
                        let cap = RE_MACRO_INCLUDE.captures(line.1).unwrap().get(1).unwrap();
                        let path: String = cap.as_str().into();

                        let start = cap.start();
                        let end = cap.end();

                        let include_path = if path.starts_with('/') {
                            let path = path.strip_prefix('/').unwrap().to_string();
                            self.pack_path.join(PathBuf::from_slash(&path))
                        } else {
                            self.file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                        };

                        self.including_files.push_back((line.0, start, end, include_path.clone()));

                        let parent_path: HashSet<PathBuf> = HashSet::from([self.file_path.clone()]);
                        IncludeFile::get_includes(include_files, &self.pack_path, include_path, &parent_path, 0);
                    });
                self.content = content;
            },
            Err(_err) => {
                error!("Unable to read file {}", self.file_path.to_str().unwrap());
            }
        }
    }

    pub fn merge_shader_file(&self, include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, file_list: &mut HashMap<String, PathBuf>) -> String {
        let mut shader_content: String = String::new();
        file_list.insert("0".to_owned(), self.file_path.clone());
        let mut file_id = 0;

        // If we are in the debug folder, do not add Optifine's macros
        let mut macro_inserted = self.pack_path.parent().unwrap().file_name().unwrap() == "debug";

        self.content.lines()
            .enumerate()
            .for_each(|line| {
                if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                    let cap = capture.get(1).unwrap();
                    let path: String = cap.as_str().into();

                    let include_path = if path.starts_with('/') {
                        let path = path.strip_prefix('/').unwrap().to_string();
                        self.pack_path.join(PathBuf::from_slash(&path))
                    } else {
                        self.file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                    };

                    if let Some(include_file) = include_files.get(&include_path) {
                        let include_content = include_file.clone().merge_include(include_files, line.1.to_string(), file_list, &mut file_id, 1);
                        shader_content += &include_content;
                        shader_content += &format!("#line {} 0\n", line.0 + 2);
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
                    if !macro_inserted &&RE_MACRO_VERSION.is_match(line.1) {
                        shader_content += OPTIFINE_MACROS;
                        shader_content += &format!("#line {} 0\n", line.0 + 2);
                        macro_inserted = true;
                    }
                }
            });

        shader_content
    }

    pub fn temp_merge_shader(file_path: &PathBuf, pack_path: &PathBuf, file_list: &mut HashMap<String, PathBuf>) -> String {
        let mut shader_content: String = String::new();
        file_list.insert("0".to_owned(), file_path.clone());
        let mut file_id = 0;

        // If we are in the debug folder, do not add Optifine's macros
        let mut macro_inserted = pack_path.parent().unwrap().file_name().unwrap() == "debug";

        let shader_reader = BufReader::new(match std::fs::File::open(file_path) {
            Ok(inner) => inner,
            Err(_err) => {
                warn!("Unable to read file"; "path" => file_path.to_str().unwrap());
                return shader_content
            }
        });

        shader_reader.lines()
            .enumerate()
            .filter_map(|line| match line.1 {
                Ok(t) => Some((line.0, t)),
                Err(_e) => None,
            })
            .for_each(|line| {
                if RE_MACRO_INCLUDE.is_match(&line.1) {
                    file_id += 1;
                    let cap = RE_MACRO_INCLUDE.captures(line.1.as_str()).unwrap().get(1).unwrap();
                    let path: String = cap.as_str().into();

                    let include_path = if path.starts_with('/') {
                        let path = path.strip_prefix('/').unwrap().to_string();
                        pack_path.join(PathBuf::from_slash(&path))
                    } else {
                        file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                    };

                    let include_content = IncludeFile::temp_merge_include(pack_path, &include_path, file_list, line.1, &mut file_id, 1);
                    shader_content += &include_content;

                    shader_content += &format!("#line {} 0\n", line.0 + 2);
                }
                else if RE_MACRO_LINE.is_match(&line.1) {
                    // Delete existing #line for correct linting
                    shader_content += "\n";
                }
                else {
                    shader_content += &line.1;
                    shader_content += "\n";
                    // If we are not in the debug folder, add Optifine's macros for correct linting
                    if RE_MACRO_VERSION.is_match(line.1.as_str()) && !macro_inserted {
                        shader_content += OPTIFINE_MACROS;
                        shader_content += &format!("#line {} 0\n", line.0 + 2);
                        macro_inserted = true;
                    }
                }
            });

        shader_content
    }
}

#[derive(Clone)]
pub struct IncludeFile {
    // File path
    file_path: PathBuf,
    // Live content for this file
    content: String,
    // The shader pack path that this file in
    pack_path: PathBuf,
    // Shader files that include this file
    included_shaders: HashSet<PathBuf>,
    // Files included in this file (line, start char, end char, file path)
    including_files: LinkedList<(usize, usize, usize, PathBuf)>,
}

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

        for file in include_file.including_files.clone() {
            Self::update_parent(include_files, &file.3, parent_file, depth + 1);
        }
    }

    pub fn get_includes(include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>,
        pack_path: &PathBuf, include_path: PathBuf, parent_file: &HashSet<PathBuf>, depth: i32
    ) {
        if depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return;
        }
        if include_files.contains_key(&include_path) {
            Self::update_parent(include_files, &include_path, parent_file, depth);
        }
        else {
            let mut include = IncludeFile {
                file_path: include_path.clone(),
                content: String::new(),
                pack_path: pack_path.clone(),
                included_shaders: parent_file.clone(),
                including_files: LinkedList::new(),
            };
            
            match read_to_string(&include_path) {
                Ok(content) => {
                    content.lines()
                        .enumerate()
                        .filter(|line| RE_MACRO_INCLUDE.is_match(line.1))
                        .for_each(|line| {
                            let cap = RE_MACRO_INCLUDE.captures(line.1).unwrap().get(1).unwrap();
                            let path: String = cap.as_str().into();

                            let start = cap.start();
                            let end = cap.end();

                            let sub_include_path = if path.starts_with('/') {
                                let path = path.strip_prefix('/').unwrap().to_string();
                                pack_path.join(PathBuf::from_slash(&path))
                            } else {
                                include_path.parent().unwrap().join(PathBuf::from_slash(&path))
                            };

                            include.including_files.push_back((line.0, start, end, sub_include_path.clone()));

                            Self::get_includes(include_files, pack_path, sub_include_path, parent_file, depth + 1);
                        });
                    include.content = content;
                },
                Err(_err) => {
                    error!("Unable to read file {}", include_path.to_str().unwrap());
                }
            }

            include_files.insert(include_path, include);
        }
    }

    pub fn update_include(&mut self, include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>) {
        self.including_files.clear();

        match read_to_string(&self.file_path) {
            Ok(content) => {
                content.lines()
                    .enumerate()
                    .filter(|line| RE_MACRO_INCLUDE.is_match(line.1))
                    .for_each(|line| {
                        let cap = RE_MACRO_INCLUDE.captures(line.1).unwrap().get(1).unwrap();
                        let path: String = cap.as_str().into();

                        let start = cap.start();
                        let end = cap.end();

                        let sub_include_path = if path.starts_with('/') {
                            let path = path.strip_prefix('/').unwrap().to_string();
                            self.pack_path.join(PathBuf::from_slash(&path))
                        } else {
                            self.file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                        };

                        self.including_files.push_back((line.0, start, end, sub_include_path.clone()));

                        Self::get_includes(include_files, &self.pack_path, sub_include_path, &self.included_shaders, 1);
                    });
                self.content = content;
            },
            Err(_err) => {
                warn!("Unable to read file"; "path" => self.file_path.to_str().unwrap());
            }
        }
    }

    pub fn merge_include(&self, include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>,
        original_content: String, file_list: &mut HashMap<String, PathBuf>, file_id: &mut i32, depth: i32
    ) -> String {
        if !self.file_path.exists() || depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            original_content + "\n"
        }
        else {
            let mut include_content: String = String::new();
            file_list.insert(file_id.to_string(), self.file_path.clone());
            include_content += &format!("#line 1 {}\n", &file_id.to_string());
            let curr_file_id = file_id.clone();

            self.content.lines()
                .enumerate()
                .for_each(|line| {
                    if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                        let cap = capture.get(1).unwrap();
                        let path: String = cap.as_str().into();

                        let include_path = if path.starts_with('/') {
                            let path = path.strip_prefix('/').unwrap().to_string();
                            self.pack_path.join(PathBuf::from_slash(&path))
                        } else {
                            self.file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                        };

                        if let Some(include_file) = include_files.get(&include_path) {
                            let sub_include_content = include_file.clone().merge_include(include_files, line.1.to_string(), file_list, file_id, 1);
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

    pub fn temp_merge_include(pack_path: &PathBuf, file_path: &PathBuf, file_list: &mut HashMap<String, PathBuf>,
        original_content: String, file_id: &mut i32, depth: i32
    ) -> String {
        if depth > 10 || !file_path.exists() {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return original_content + "\n";
        }
        let mut include_content = String::new();
        file_list.insert(file_id.to_string(), file_path.clone());
        include_content += &format!("#line 1 {}\n", &file_id.to_string());
        let curr_file_id = file_id.clone();

        let include_reader = BufReader::new(match std::fs::File::open(&file_path) {
            Ok(inner) => inner,
            Err(_err) => {
                warn!("Unable to read file"; "path" => file_path.to_str().unwrap());
                return original_content + "\n"
            }
        });
        include_reader.lines()
            .enumerate()
            .filter_map(|line| match line.1 {
                Ok(t) => Some((line.0, t)),
                Err(_e) => None,
            })
            .for_each(|line| {
                if RE_MACRO_INCLUDE.is_match(&line.1) {
                    *file_id += 1;
                    let cap = RE_MACRO_INCLUDE.captures(line.1.as_str()).unwrap().get(1).unwrap();
                    let path: String = cap.as_str().into();

                    let include_path = if path.starts_with('/') {
                        let path = path.strip_prefix('/').unwrap().to_string();
                        pack_path.join(PathBuf::from_slash(&path))
                    } else {
                        file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                    };

                    let sub_include_content = Self::temp_merge_include(pack_path, &include_path, file_list, line.1, file_id, 1);
                    include_content += &sub_include_content;

                    include_content += &format!("#line {} {}\n", line.0 + 2, curr_file_id);
                }
                else if RE_MACRO_LINE.is_match(&line.1) {
                    // Delete existing #line for correct linting
                    include_content += "\n";
                }
                else {
                    include_content += &line.1;
                    include_content += "\n";
                }
            });
        include_content
    }
}
