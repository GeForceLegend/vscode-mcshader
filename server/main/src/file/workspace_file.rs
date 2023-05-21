use super::*;

impl WorkspaceFile {
    /// Sending the standalone clone data of a shader file to update its include.
    /// Since workspace_files may get amortized, using reference to workspace file inside it is not allowed.
    pub fn update_include(
        workspace_files: &mut HashMap<PathBuf, WorkspaceFile>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        mut old_including_files: HashSet<PathBuf>, content: &str, pack_path: &PathBuf, file_path: &PathBuf, mut depth: i32,
    ) -> *const WorkspaceFile {
        if depth <= 10 {
            depth += 1;
            let mut including_files = vec![];

            content
                .split_terminator('\n')
                .enumerate()
                .filter_map(|(line, content)| RE_MACRO_INCLUDE.captures(content).map(|captures| (line, captures)))
                .for_each(|(line, captures)| {
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
                            } else if let Some(temp_file) = temp_files.remove(&include_path) {
                                temp_file.into_workspace_file(
                                    workspace_files,
                                    temp_files,
                                    parser,
                                    pack_path,
                                    &include_path,
                                    file_path,
                                    depth,
                                );
                            } else {
                                Self::new_include(workspace_files, temp_files, parser, pack_path, &include_path, file_path, depth);
                            }
                            including_files.push((line, start, end, include_path));
                        }
                        Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
                    }
                });
            // They are removed from including list of this file. Let's remove this file from their parent list.
            old_including_files
                .iter()
                .filter_map(|including_path| workspace_files.get(including_path))
                .for_each(|including_file| {
                    including_file.included_files.borrow_mut().remove(file_path);
                });
            let workspace_file = workspace_files.get(file_path).unwrap();
            *workspace_file.including_files.borrow_mut() = including_files;
            workspace_file as *const Self
        } else {
            workspace_files.get(file_path).unwrap() as *const Self
        }
    }

    pub fn new_shader(
        workspace_files: &mut HashMap<PathBuf, WorkspaceFile>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        pack_path: &PathBuf, file_path: &PathBuf,
    ) {
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
                // This will never be used since we have ensured the extension through basic shaders regex.
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
            // Clone the content so they can be used alone.
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

        Self::update_include(
            workspace_files,
            temp_files,
            parser,
            HashSet::new(),
            &content,
            pack_path,
            file_path,
            0,
        );
    }

    pub fn new_include(
        workspace_files: &mut HashMap<PathBuf, WorkspaceFile>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        pack_path: &PathBuf, file_path: &PathBuf, parent_path: &Path, depth: i32,
    ) {
        let include_file = WorkspaceFile {
            file_type: RefCell::new(gl::NONE),
            pack_path: pack_path.clone(),
            content: RefCell::new(String::new()),
            tree: RefCell::new(parser.parse("", None).unwrap()),
            line_mapping: RefCell::new(vec![]),
            included_files: RefCell::new(HashSet::from([parent_path.to_path_buf()])),
            including_files: RefCell::new(vec![]),
        };
        if file_path.exists() {
            include_file.update_from_disc(parser, file_path);
            // Clone the content so they can be used alone.
            let content = include_file.content.borrow().clone();

            workspace_files.insert(file_path.clone(), include_file);

            Self::update_include(
                workspace_files,
                temp_files,
                parser,
                HashSet::new(),
                &content,
                pack_path,
                file_path,
                depth,
            );
        } else {
            *include_file.file_type.borrow_mut() = gl::INVALID_ENUM;
            error!("Include file {} not found in workspace!", file_path.to_str().unwrap());
            workspace_files.insert(file_path.clone(), include_file);
        }
    }

    pub fn merge_file(
        &self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, file_list: &mut HashMap<String, Url>, shader_content: &mut String,
        file_path: &PathBuf, file_id: &mut i32, mut depth: u8,
    ) -> bool {
        if !file_path.exists() || depth > 10 {
            return false;
        }
        depth += 1;
        *file_id += 1;
        let curr_file_id = Buffer::new().format(*file_id).to_owned();
        let file_name = file_path.to_str().unwrap();
        generate_line_macro(shader_content, 1, &curr_file_id, file_name);
        shader_content.push('\n');

        let content = self.content.borrow();
        let line_mapping = self.line_mapping.borrow();
        let including_files = self.including_files.borrow();
        let mut start_index = 0;

        including_files
            .iter()
            .filter_map(|(line, _, _, include_path)| workspace_files.get(include_path).map(|workspace_file| (line, include_path, workspace_file)))
            .for_each(|(line, include_path, workspace_file)| {
                let start = line_mapping.get(*line).unwrap();
                let end = line_mapping.get(line + 1).unwrap();

                let before_content = unsafe { content.get_unchecked(start_index..*start) };
                push_str_without_line(shader_content, before_content);
                start_index = end - 1;

                if workspace_file.merge_file(workspace_files, file_list, shader_content, include_path, file_id, depth) {
                    generate_line_macro(shader_content, line + 2, &curr_file_id, file_name);
                } else {
                    shader_content.push_str(unsafe { content.get_unchecked(*start..start_index) });
                }
            });
        push_str_without_line(shader_content, unsafe { content.get_unchecked(start_index..) });
        shader_content.push('\n');
        file_list.insert(curr_file_id, Url::from_file_path(file_path).unwrap());

        true
    }

    pub fn get_base_shaders<'a>(
        &'a self, workspace_files: &'a HashMap<PathBuf, WorkspaceFile>, base_shaders: &mut HashMap<&'a PathBuf, &'a WorkspaceFile>,
        file_path: &'a PathBuf, mut depth: u8,
    ) {
        depth += 1;
        let file_type = *self.file_type.borrow();
        if file_type != gl::NONE && file_type != gl::INVALID_ENUM {
            // workspace_files would not change when linting shaders. This should be safe.
            base_shaders.insert(file_path, self);
        }
        if depth < 10 {
            self.included_files
                .borrow()
                .iter()
                .filter_map(|included_path| workspace_files.get_key_value(included_path))
                .for_each(|(included_path, workspace_file)| {
                    workspace_file.get_base_shaders(workspace_files, base_shaders, included_path, depth);
                });
        }
    }

    pub fn get_base_shader_pathes(
        &self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, base_shaders: &mut HashSet<PathBuf>, file_path: &Path, mut depth: u8,
    ) {
        depth += 1;
        let file_type = *self.file_type.borrow();
        if file_type != gl::NONE && file_type != gl::INVALID_ENUM {
            // workspace_files would not change when linting shaders. This should be safe.
            base_shaders.insert(file_path.to_path_buf());
        }
        if depth < 10 {
            self.included_files
                .borrow()
                .iter()
                .filter_map(|included_path| workspace_files.get_key_value(included_path))
                .for_each(|(included_path, workspace_file)| {
                    workspace_file.get_base_shader_pathes(workspace_files, base_shaders, included_path, depth);
                });
        }
    }

    pub fn clear(&self, parser: &mut Parser) {
        *self.file_type.borrow_mut() = gl::INVALID_ENUM;
        self.content.borrow_mut().clear();
        *self.tree.borrow_mut() = parser.parse("", None).unwrap();
        self.line_mapping.borrow_mut().clear();
        self.including_files.borrow_mut().clear();
    }

    pub fn including_pathes(&self) -> HashSet<PathBuf> {
        self.including_files()
            .borrow()
            .iter()
            .map(|including_data| including_data.3.clone())
            .collect::<HashSet<_>>()
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
