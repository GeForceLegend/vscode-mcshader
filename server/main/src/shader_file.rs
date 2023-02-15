use std::{
    collections::{HashMap, HashSet, LinkedList},
    path::{PathBuf},
    io::{BufReader, BufRead},
    sync::MutexGuard,
};

use logging::warn;
use path_slash::PathBufExt;

use slog_scope::error;

use crate::constant::{
    DEFAULT_INCLUDE_FILE,
    RE_MACRO_INCLUDE,
    RE_MACRO_LINE,
    RE_MACRO_VERSION,
    OPTIFINE_MACROS,
};

fn load_cursor_content(cursor_content: Option<&(usize, usize, usize, PathBuf)>) -> &(usize, usize, usize, PathBuf) {
    match cursor_content {
        Some(include_file) => include_file,
        None => &DEFAULT_INCLUDE_FILE,
    }
}

#[derive(Clone)]
pub struct ShaderFile {
    // File path
    file_path: PathBuf,
    // Type of the shader
    file_type: gl::types::GLenum,
    // The shader pack path that this file in
    pack_path: PathBuf,
    // Files included in this file (line, start char, end char, file path)
    including_files: LinkedList<(usize, usize, usize, PathBuf)>,
}

impl ShaderFile {
    pub fn file_type(&self) -> &gl::types::GLenum {
        &self.file_type
    }

    pub fn including_files(&self) -> &LinkedList<(usize, usize, usize, PathBuf)> {
        &self.including_files
    }

    pub fn clear_including_files(&mut self) {
        self.including_files.clear();
    }

    pub fn new(pack_path: &PathBuf, file_path: &PathBuf) -> ShaderFile {
        ShaderFile {
            file_path: file_path.clone(),
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

        let parent_path: HashSet<PathBuf> = HashSet::from([self.file_path.clone()]);

        let shader_reader = BufReader::new(match std::fs::File::open(&self.file_path) {
            Ok(inner) => inner,
            Err(_err) => {
                warn!("Unable to read file"; "path" => self.file_path.to_str().unwrap());
                return
            }
        });
        shader_reader.lines()
            .enumerate()
            .filter_map(|line| match line.1 {
                Ok(t) => Some((line.0, t)),
                Err(_e) => None,
            })
            .filter(|line| RE_MACRO_INCLUDE.is_match(line.1.as_str()))
            .for_each(|line| {
                let cap = RE_MACRO_INCLUDE.captures(line.1.as_str()).unwrap().get(1).unwrap();
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

                IncludeFile::get_includes(include_files, &self.pack_path, include_path, &parent_path, 0);
            });
    }

    pub fn merge_shader_file(&self, include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, file_list: &mut HashMap<String, PathBuf>) -> String {
        let mut shader_content: String = String::new();
        file_list.insert("0".to_owned(), self.file_path.clone());
        let mut file_id = 0;

        // Get a cursor pointed to the first position of LinkedList, and we can get data without have to clone one and pop_front()!
        let mut including_files = self.including_files.cursor_front();
        let mut next_include_file = load_cursor_content(including_files.current());

        // If we are in the debug folder, do not add Optifine's macros
        let mut macro_inserted = self.pack_path.parent().unwrap().file_name().unwrap() == "debug";

        let shader_reader = BufReader::new(match std::fs::File::open(&self.file_path) {
            Ok(inner) => inner,
            Err(_err) => {
                warn!("Unable to read file"; "path" => self.file_path.to_str().unwrap());
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
                if line.0 == next_include_file.0 {
                    let include_file = include_files.get(&next_include_file.3);
                    match include_file {
                        Some(include) => {
                            let include_file = include.clone();
                            file_id += 1;
                            let include_content = include_file.merge_include(include_files, line.1, file_list, &mut file_id, 1);
                            shader_content += &include_content;
                            // Move cursor to the next position and get the value
                            including_files.move_next();
                            next_include_file = load_cursor_content(including_files.current());

                            shader_content += &format!("#line {} 0\n", line.0 + 2);
                        },
                        None => {
                            shader_content += &line.1;
                            shader_content += "\n";
                        }
                    };
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
    // The shader pack path that this file in
    pack_path: PathBuf,
    // Shader files that include this file
    included_shaders: HashSet<PathBuf>,
    // Files included in this file (line, start char, end char, file path)
    including_files: LinkedList<(usize, usize, usize, PathBuf)>,
}

impl IncludeFile {
    pub fn included_shaders(&self) -> &HashSet<PathBuf> {
        &self.included_shaders
    }

    pub fn included_shaders_mut(&mut self) -> &mut HashSet<PathBuf> {
        &mut self.included_shaders
    }

    pub fn including_files(&self) -> &LinkedList<(usize, usize, usize, PathBuf)> {
        &self.including_files
    }

    pub fn update_parent(include_path: &PathBuf, parent_file: &HashSet<PathBuf>, include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, depth: i32
    ) {
        if depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return;
        }
        let include_file = include_files.get_mut(include_path).unwrap();
        include_file.included_shaders.extend(parent_file.clone());

        for file in include_file.including_files.clone() {
            Self::update_parent(&file.3, parent_file, include_files, depth + 1);
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
            Self::update_parent(&include_path, parent_file, include_files, depth);
        }
        else {
            let mut include = IncludeFile {
                file_path: include_path.clone(),
                pack_path: pack_path.clone(),
                included_shaders: parent_file.clone(),
                including_files: LinkedList::new(),
            };

            if include_path.exists() {
                let reader = BufReader::new(std::fs::File::open(&include_path).unwrap());
                reader.lines()
                    .enumerate()
                    .filter_map(|line| match line.1 {
                        Ok(t) => Some((line.0, t)),
                        Err(_e) => None,
                    })
                    .filter(|line| RE_MACRO_INCLUDE.is_match(line.1.as_str()))
                    .for_each(|line| {
                        let cap = RE_MACRO_INCLUDE.captures(line.1.as_str()).unwrap().get(1).unwrap();
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
            }
            else {
                error!("cannot find include file {}", include_path.to_str().unwrap());
            }

            include_files.insert(include_path, include);
        }
    }

    pub fn update_include(&mut self, include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>) {
        self.including_files.clear();

        let include_reader = BufReader::new(match std::fs::File::open(&self.file_path){
            Ok(inner) => inner,
            Err(_err) => {
                warn!("Unable to read file"; "path" => self.file_path.to_str().unwrap());
                return;
            }
        });
        include_reader.lines()
            .enumerate()
            .filter_map(|line| match line.1 {
                Ok(t) => Some((line.0, t)),
                Err(_e) => None,
            })
            .filter(|line| RE_MACRO_INCLUDE.is_match(line.1.as_str()))
            .for_each(|line| {
                let cap = RE_MACRO_INCLUDE.captures(line.1.as_str()).unwrap().get(1).unwrap();
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

            // Get a cursor pointed to the first position of LinkedList, and we can get data without have to clone one and pop_front()!
            let mut including_files = self.including_files.cursor_front();
            let mut next_include_file = load_cursor_content(including_files.current());

            let include_reader = BufReader::new(match std::fs::File::open(&self.file_path) {
                Ok(inner) => inner,
                Err(_err) => {
                    warn!("Unable to read file"; "path" => self.file_path.to_str().unwrap());
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
                    if line.0 == next_include_file.0 {
                        let include_file = include_files.get(&next_include_file.3).unwrap().clone();
                        *file_id += 1;
                        let sub_include_content = include_file.merge_include(include_files, line.1, file_list, file_id, depth + 1);
                        include_content += &sub_include_content;
                        // Move cursor to the next position and get the value
                        including_files.move_next();
                        next_include_file = load_cursor_content(including_files.current());

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

    pub fn temp_search_include(pack_path: &PathBuf, file_path: &PathBuf) -> LinkedList<(usize, usize, usize, PathBuf)> {
        let mut include_list = LinkedList::new();

        let reader = BufReader::new(match std::fs::File::open(&file_path) {
            Ok(inner) => inner,
            Err(_err) => {
                return include_list
            }
        });
        reader.lines()
            .enumerate()
            .filter_map(|line| match line.1 {
                Ok(t) => Some((line.0, t)),
                Err(_e) => None,
            })
            .filter(|line| RE_MACRO_INCLUDE.is_match(line.1.as_str()))
            .for_each(|line| {
                let cap = RE_MACRO_INCLUDE.captures(line.1.as_str()).unwrap().get(1).unwrap();
                let path: String = cap.as_str().into();

                let start = cap.start();
                let end = cap.end();

                let sub_include_path = if path.starts_with('/') {
                    let path = path.strip_prefix('/').unwrap().to_string();
                    pack_path.join(PathBuf::from_slash(&path))
                } else {
                    file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                };

                include_list.push_back((line.0, start, end, sub_include_path.clone()));
            });

        include_list
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
