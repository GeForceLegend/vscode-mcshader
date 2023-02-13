use std::{
    collections::{HashMap, HashSet, LinkedList},
    path::{PathBuf},
    io::{BufReader, BufRead},
};

use path_slash::PathBufExt;
use regex::Regex;

use lazy_static::lazy_static;
use slog_scope::error;

const OPTIFINE_MACROS: &str = "#define MC_VERSION 11900
#define MC_GL_VERSION 320
#define MC_GLSL_VERSION 150
#define MC_OS_WINDOWS
#define MC_GL_VENDOR_NVIDIA
#define MC_GL_RENDERER_GEFORCE
#define MC_NORMAL_MAP
#define MC_SPECULAR_MAP
#define MC_RENDER_QUALITY 1.0
#define MC_SHADOW_QUALITY 1.0
#define MC_HAND_DEPTH 0.125
#define MC_RENDER_STAGE_NONE 0
#define MC_RENDER_STAGE_SKY 1
#define MC_RENDER_STAGE_SUNSET 2
#define MC_RENDER_STAGE_SUN 4
#define MC_RENDER_STAGE_CUSTOM_SKY 3
#define MC_RENDER_STAGE_MOON 5
#define MC_RENDER_STAGE_STARS 6
#define MC_RENDER_STAGE_VOID 7
#define MC_RENDER_STAGE_TERRAIN_SOLID 8
#define MC_RENDER_STAGE_TERRAIN_CUTOUT_MIPPED 9
#define MC_RENDER_STAGE_TERRAIN_CUTOUT 10
#define MC_RENDER_STAGE_ENTITIES 11
#define MC_RENDER_STAGE_BLOCK_ENTITIES 12
#define MC_RENDER_STAGE_DESTROY 13
#define MC_RENDER_STAGE_OUTLINE 14
#define MC_RENDER_STAGE_DEBUG 15
#define MC_RENDER_STAGE_HAND_SOLID 16
#define MC_RENDER_STAGE_TERRAIN_TRANSLUCENT 17
#define MC_RENDER_STAGE_TRIPWIRE 18
#define MC_RENDER_STAGE_PARTICLES 19
#define MC_RENDER_STAGE_CLOUDS 20
#define MC_RENDER_STAGE_RAIN_SNOW 21
#define MC_RENDER_STAGE_WORLD_BORDER 22
#define MC_RENDER_STAGE_HAND_TRANSLUCENT 23
";

lazy_static! {
    static ref RE_MACRO_INCLUDE: Regex = Regex::new(r#"^(?:\s)*?(?:#include) "(.+)"\r?"#).unwrap();
    static ref RE_MACRO_VERSION: Regex = Regex::new(r#"^(?:\s)*?(?:#version) \r?"#).unwrap();
    static ref RE_MACRO_LINE: Regex = Regex::new(r#"^(?:\s)*?(?:#line) \r?"#).unwrap();
    static ref DEFAULT_INCLUDE_FILE: (usize, usize, usize, PathBuf) = (usize::from(u16::MAX), usize::from(u16::MAX), usize::from(u16::MAX), PathBuf::from("/"));
}

fn load_cursor_content(cursor_content: Option<&(usize, usize, usize, PathBuf)>) -> &(usize, usize, usize, PathBuf) {
    match cursor_content {
        Some(include_file) => include_file,
        None => &DEFAULT_INCLUDE_FILE,
    }
}

#[derive(Clone)]
pub struct ShaderFile {
    // File path
    path: PathBuf,
    // Type of the shader
    file_type: gl::types::GLenum,
    // The work space that this file in
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

    pub fn new(pack_path: &PathBuf, path: &PathBuf) -> ShaderFile {
        ShaderFile {
            path: path.clone(),
            file_type: gl::NONE,
            pack_path: pack_path.clone(),
            including_files: LinkedList::new(),
        }
    }

    pub fn read_file (&mut self, include_files: &mut HashMap<PathBuf, IncludeFile>) {
        let extension = self.path.extension().unwrap();
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

        let parent_path: HashSet<PathBuf> = HashSet::from([self.path.clone()]);

        let reader = BufReader::new(std::fs::File::open(&self.path).unwrap());
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

                let include_path = if path.starts_with('/') {
                    let path = path.strip_prefix('/').unwrap().to_string();
                    self.pack_path.join(PathBuf::from_slash(&path))
                } else {
                    self.path.parent().unwrap().join(PathBuf::from_slash(&path))
                };

                self.including_files.push_back((line.0, start, end, include_path.clone()));

                IncludeFile::get_includes(&self.pack_path, include_path, &parent_path, include_files, 0);
            });
    }

    pub fn merge_shader_file(&self, include_files: &HashMap<PathBuf, IncludeFile>, file_list: &mut HashMap<String, PathBuf>) -> String {
        let mut shader_content: String = String::new();
        file_list.insert("0".to_owned(), self.path.clone());
        let mut file_id = 0;

        // Get a cursor pointed to the first position of LinkedList, and we can get data without have to clone one and pop_front()!
        let mut including_files = self.including_files.cursor_front();
        let mut next_include_file = load_cursor_content(including_files.current());

        // If we are in the debug folder, do not add Optifine's macros
        let mut macro_inserted = self.pack_path.parent().unwrap().file_name().unwrap() == "debug";

        let shader_reader = BufReader::new(std::fs::File::open(&self.path).unwrap());
        shader_reader.lines()
            .enumerate()
            .filter_map(|line| match line.1 {
                Ok(t) => Some((line.0, t)),
                Err(_e) => None,
            })
            .for_each(|line| {
                if line.0 == next_include_file.0 {
                    let include_file = include_files.get(&next_include_file.3).unwrap();
                    file_id += 1;
                    let include_content = include_file.merge_include(line.1, include_files, file_list, &mut file_id, 1);
                    shader_content += &include_content;
                    // Move cursor to the next position and get the value
                    including_files.move_next();
                    next_include_file = load_cursor_content(including_files.current());

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
    path: PathBuf,
    // The work space that this file in
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

    pub fn update_parent(include_path: &PathBuf, parent_file: &HashSet<PathBuf>, include_files: &mut HashMap<PathBuf, IncludeFile>, depth: i32) {
        if depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return;
        }
        let mut include_file = include_files.remove(include_path).unwrap();

        for file in &include_file.including_files {
            Self::update_parent(&file.3, parent_file, include_files, depth + 1);
        }

        include_file.included_shaders.extend(parent_file.clone());
        include_files.insert(include_path.clone(), include_file);
    }

    pub fn get_includes(pack_path: &PathBuf, include_path: PathBuf, parent_file: &HashSet<PathBuf>, include_files: &mut HashMap<PathBuf, IncludeFile>, depth: i32) {
        if depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return;
        }
        if include_files.contains_key(&include_path) {
            let mut include = include_files.remove(&include_path).unwrap();
            include.included_shaders.extend(parent_file.clone());
            for file in &include.including_files {
                Self::update_parent(&file.3, parent_file, include_files, depth + 1);
            }
            include_files.insert(include_path, include);
        }
        else {
            let mut include = IncludeFile {
                path: include_path.clone(),
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

                        Self::get_includes(pack_path, sub_include_path, parent_file, include_files, depth + 1);
                    });
            }
            else {
                error!("cannot find include file {}", include_path.to_str().unwrap());
            }

            include_files.insert(include_path, include);
        }
    }

    pub fn update_include(&mut self, include_files: &mut HashMap<PathBuf, IncludeFile>) {
        self.including_files.clear();

        let reader = BufReader::new(std::fs::File::open(&self.path).unwrap());
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
                    self.pack_path.join(PathBuf::from_slash(&path))
                } else {
                    self.path.parent().unwrap().join(PathBuf::from_slash(&path))
                };

                self.including_files.push_back((line.0, start, end, sub_include_path.clone()));

                Self::get_includes(&self.pack_path, sub_include_path, &self.included_shaders, include_files, 1);
            });
    }

    pub fn merge_include(&self, original_content: String, include_files: &HashMap<PathBuf, IncludeFile>, file_list: &mut HashMap<String, PathBuf>, file_id: &mut i32, depth: i32) -> String {
        if !self.path.exists() || depth > 10 {
            original_content + "\n"
        }
        else {
            let mut include_content: String = String::new();
            file_list.insert(file_id.to_string(), self.path.clone());
            include_content += &format!("#line 1 {}\n", &file_id.to_string());
            let curr_file_id = file_id.clone();

            // Get a cursor pointed to the first position of LinkedList, and we can get data without have to clone one and pop_front()!
            let mut including_files = self.including_files.cursor_front();
            let mut next_include_file = load_cursor_content(including_files.current());

            let shader_reader = BufReader::new(std::fs::File::open(&self.path).unwrap());
            shader_reader.lines()
                .enumerate()
                .filter_map(|line| match line.1 {
                    Ok(t) => Some((line.0, t)),
                    Err(_e) => None,
                })
                .for_each(|line| {
                    if line.0 == next_include_file.0 {
                        let include_file = include_files.get(&next_include_file.3).unwrap();
                        *file_id += 1;
                        let sub_include_content = include_file.merge_include(line.1, include_files, file_list, file_id, depth + 1);
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
}
