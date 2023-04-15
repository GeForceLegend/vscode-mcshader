use super::*;

impl WorkspaceFile {
    pub fn update_include<'a>(
        workspace_files: &'a mut HashMap<PathBuf, WorkspaceFile>, parser: &mut Parser, mut old_including_files: HashSet<PathBuf>,
        content: &str, pack_path: &PathBuf, file_path: &PathBuf, mut depth: i32
    ) -> *const WorkspaceFile {
        if depth <= 10 {
            depth += 1;
            let mut including_files = vec![];

            content.split_terminator("\n").enumerate().filter_map(|(line, content)| {
                match RE_MACRO_INCLUDE.captures(content) {
                    Some(captures) => Some((line, captures)),
                    None => None,
                }
            }).for_each(|(line, captures)| {
                let include_content = captures.get(1).unwrap();
                let path = include_content.as_str();
                match include_path_join(pack_path, file_path, path) {
                    Ok(include_path) => {
                        let start = include_content.start();
                        let end = include_content.end();
                        let already_includes = old_including_files.remove(&include_path);

                        if let Some(workspace_file) = workspace_files.get(&include_path) {
                            // File exists in workspace_files. If this is already included before modification, no need to update its includes.
                            if !already_includes {
                                workspace_file.included_files.borrow_mut().insert(file_path.clone());
                            }
                        } else {
                            Self::new_include(workspace_files, parser, pack_path, &include_path, file_path, depth);
                        }
                        including_files.push((line, start, end, include_path));
                    },
                    Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
                }
            });
            // They are removed from including list of this file. Let's move this file from their parent list.
            old_including_files.iter().filter_map(|including_path| workspace_files.get(including_path)).for_each(|including_file| {
                including_file.included_files.borrow_mut().remove(file_path);
            });
            let workspace_file = workspace_files.get(file_path).unwrap();
            *workspace_file.including_files.borrow_mut() = including_files;
            workspace_file as *const Self
        } else {
            workspace_files.get(file_path).unwrap() as *const Self
        }
    }

    pub fn new_shader(workspace_files: &mut HashMap<PathBuf, WorkspaceFile>, parser: &mut Parser, pack_path: &PathBuf, file_path: &PathBuf) {
        let extension = file_path.extension().unwrap();
        let file_type = {
            if extension == "fsh" {
                gl::FRAGMENT_SHADER
            } else if extension == "vsh" {
                gl::VERTEX_SHADER
            } else if extension == "gsh" {
                gl::GEOMETRY_SHADER
            } else if extension == "csh" {
                gl::COMPUTE_SHADER
            } else {
                // This will never be used since we have ensured the extension through basic shaders list.
                gl::NONE
            }
        };
        let content;
        if let Some(workspace_file) = workspace_files.get_mut(file_path) {
            // Existing as some file's include
            let mut existing_file_type = workspace_file.file_type.borrow_mut();
            if *existing_file_type != gl::INVALID_ENUM {
                // File already scanned. Just change its type to shaders.
                *existing_file_type = file_type;
                return;
            }
            *existing_file_type = file_type;
            workspace_file.update_from_disc(parser, file_path);
            content = workspace_file.content.borrow().clone();
        } else {
            let shader_file = WorkspaceFile {
                file_type: RefCell::new(file_type),
                pack_path: pack_path.clone(),
                content: RefCell::new(String::new()),
                tree: RefCell::new(parser.parse("", None).unwrap()),
                line_mapping: RefCell::new(vec![]),
                included_files: RefCell::new(HashSet::new()),
                including_files: RefCell::new(vec![]),
            };
            shader_file.update_from_disc(parser, file_path);
            // Clone the content so they can be used alone.
            content = shader_file.content.borrow().clone();

            // Insert the shader file into workspace file list and takes the place. Recursions in after call will only modify its included_files.
            workspace_files.insert(file_path.clone(), shader_file);
        }

        Self::update_include(workspace_files, parser, HashSet::new(), &content, pack_path, file_path, 0);
    }

    pub fn new_include(
        workspace_files: &mut HashMap<PathBuf, WorkspaceFile>, parser: &mut Parser, pack_path: &PathBuf, file_path: &PathBuf, parent_path: &PathBuf, depth: i32
    ) {
        let mut include_file = WorkspaceFile {
            file_type: RefCell::new(gl::INVALID_ENUM),
            pack_path: pack_path.clone(),
            content: RefCell::new(String::new()),
            tree: RefCell::new(parser.parse("", None).unwrap()),
            line_mapping: RefCell::new(vec![]),
            included_files: RefCell::new(HashSet::from([parent_path.clone()])),
            including_files: RefCell::new(vec![]),
        };
        if file_path.exists() {
            include_file.file_type = RefCell::new(gl::NONE);
            include_file.update_from_disc(parser, file_path);
            // Clone the content so they can be used alone.
            let content = include_file.content.borrow().clone();

            workspace_files.insert(file_path.clone(), include_file);

            Self::update_include(workspace_files, parser, HashSet::new(), &content, pack_path, file_path, depth);
        } else {
            error!("File not found in system! File: {}", file_path.to_str().unwrap());
            workspace_files.insert(file_path.clone(), include_file);
        }
    }

    pub fn merge_file(
        &self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, file_list: &mut HashMap<String, Url>,
        shader_content: &mut String, file_path: &PathBuf, file_id: &mut i32, mut depth: u8
    ) -> bool {
        if !file_path.exists() || depth > 10 {
            return false;
        }
        depth += 1;
        *file_id += 1;
        let curr_file_id = Buffer::new().format(*file_id).to_owned();
        let file_name = file_path.to_str().unwrap();
        shader_content.push_str(&generate_line_macro(1, &curr_file_id, file_name));
        shader_content.push('\n');

        let content = self.content.borrow();
        let line_mapping = self.line_mapping.borrow();
        let including_files = self.including_files.borrow();
        let mut start_index = 0;

        for (line, _start, _end, include_path) in including_files.iter() {
            if let Some(workspace_file) = workspace_files.get(include_path) {
                let start = line_mapping.get(*line).unwrap();
                let end = line_mapping.get(line + 1).unwrap();

                let before_content = unsafe { content.get_unchecked(start_index..*start) };
                shader_content.push_str(before_content);
                start_index = *end - 1;

                if workspace_file.merge_file(workspace_files, file_list, shader_content, include_path, file_id, depth) {
                    shader_content.push_str(&generate_line_macro(line + 2, &curr_file_id, file_name));
                } else {
                    shader_content.push_str(unsafe { content.get_unchecked(*start..start_index) });
                }
            }
        }
        shader_content.push_str(unsafe { content.get_unchecked(start_index..) });
        shader_content.push('\n');
        file_list.insert(curr_file_id, Url::from_file_path(file_path).unwrap());

        true
    }

    pub fn get_base_shaders<'a>(
        &'a self, workspace_files: &'a HashMap<PathBuf, WorkspaceFile>, base_shaders: &mut HashMap<PathBuf, &'a WorkspaceFile>,
        file_path: &PathBuf, mut depth: u8
    ) {
        if depth < 10 {
            depth += 1;
            let file_type = *self.file_type.borrow();
            if file_type != gl::NONE && file_type != gl::INVALID_ENUM {
                // workspace_files would not change when linting shaders. This should be safe.
                base_shaders.insert(file_path.clone(), self);
            }
            for included_path in self.included_files.borrow().iter() {
                if let Some(workspace_file) = workspace_files.get(included_path) {
                    workspace_file.get_base_shaders(workspace_files, base_shaders, included_path, depth);
                }
            }
        }
    }

    pub fn get_base_shader_pathes(
        &self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, base_shaders: &mut HashSet<PathBuf>,
        file_path: &PathBuf, mut depth: u8
    ) {
        if depth < 10 {
            depth += 1;
            let file_type = *self.file_type.borrow();
            if file_type != gl::NONE && file_type != gl::INVALID_ENUM {
                // workspace_files would not change when linting shaders. This should be safe.
                base_shaders.insert(file_path.clone());
            }
            for included_path in self.included_files.borrow().iter() {
                if let Some(workspace_file) = workspace_files.get(included_path) {
                    workspace_file.get_base_shader_pathes(workspace_files, base_shaders, included_path, depth);
                }
            }
        }
    }

    pub fn clear(&self, parser: &mut Parser) {
        *self.file_type.borrow_mut() = gl::INVALID_ENUM;
        *self.content.borrow_mut() = String::new();
        *self.tree.borrow_mut() = parser.parse("", None).unwrap();
        *self.line_mapping.borrow_mut() = vec![];
        *self.including_files.borrow_mut() = vec![];
    }
}

impl File for WorkspaceFile {
    fn file_type(&self) -> &RefCell<u32> {
        &self.file_type
    }

    fn pack_path(&self) -> &PathBuf {
        &self.pack_path
    }

    fn content(&self) -> &RefCell<String> {
        &self.content
    }

    fn tree(&self) -> &RefCell<Tree> {
        &self.tree
    }

    fn line_mapping(&self) -> &RefCell<Vec<usize>> {
        &self.line_mapping
    }

    fn included_files(&self) -> &RefCell<HashSet<PathBuf>> {
        &self.included_files
    }

    fn including_files(&self) -> &RefCell<Vec<(usize, usize, usize, PathBuf)>> {
        &self.including_files
    }
}
