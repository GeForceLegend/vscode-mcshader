use super::*;

impl WorkspaceFile {
    pub fn pack_path(&self) -> &Rc<PathBuf> {
        &self.pack_path
    }

    pub fn included_files(&self) -> &RefCell<HashSet<PathBuf>> {
        &self.included_files
    }

    pub fn parent_shaders(&self) -> &RefCell<HashSet<PathBuf>> {
        &self.parent_shaders
    }

    pub fn diagnostics(&self) -> &RefCell<HashMap<PathBuf, Vec<Diagnostic>>> {
        &self.diagnostics
    }

    pub fn new(parser: &mut Parser, file_type: u32, pack_path: &Rc<PathBuf>, parent_shaders: HashSet<PathBuf>) -> Self {
        Self {
            file_type: RefCell::new(file_type),
            pack_path: pack_path.clone(),
            content: RefCell::new(String::new()),
            tree: RefCell::new(parser.parse("", None).unwrap()),
            line_mapping: RefCell::new(vec![]),
            included_files: RefCell::new(HashSet::new()),
            including_files: RefCell::new(vec![]),
            parent_shaders: RefCell::new(parent_shaders),
            diagnostics: RefCell::new(HashMap::new()),
        }
    }

    fn extend_shader_list(&self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, parent_shaders: &HashSet<PathBuf>, mut depth: i32) {
        self.parent_shaders.borrow_mut().extend(parent_shaders.iter().cloned());

        if depth < 10 {
            depth += 1;
            self.including_files
                .borrow()
                .iter()
                .map(|(_, _, _, including_path)| including_path)
                .collect::<HashSet<_>>()
                .into_iter()
                .filter_map(|including_path| workspace_files.get(including_path))
                .for_each(|including_file| including_file.extend_shader_list(workspace_files, parent_shaders, depth));
        }
    }

    fn update_shader_list(&self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, may_removed: &HashSet<PathBuf>, mut depth: i32) {
        {
            let mut new_parent_shaders = self.parent_shaders.borrow_mut();
            new_parent_shaders.clear();
            self.included_files
                .borrow()
                .iter()
                .filter_map(|included_path| workspace_files.get(included_path))
                .for_each(|workspace_file| new_parent_shaders.extend(workspace_file.parent_shaders.borrow().iter().cloned()));

            let mut diagnostics = self.diagnostics.borrow_mut();
            may_removed.difference(&new_parent_shaders).for_each(|deleted_path| {
                diagnostics.remove(deleted_path);
            });
        }

        if depth < 10 {
            depth += 1;
            self.including_files
                .borrow()
                .iter()
                .map(|(_, _, _, including_path)| including_path)
                .collect::<HashSet<_>>()
                .into_iter()
                .filter_map(|including_path| workspace_files.get(including_path))
                .for_each(|including_file| including_file.update_shader_list(workspace_files, may_removed, depth));
        }
    }

    /// Sending the standalone clone data of a shader file to update its include.
    /// Since workspace_files may get amortized, using reference to workspace file inside it is not allowed.
    pub fn update_include(
        workspace_files: &mut HashMap<PathBuf, WorkspaceFile>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        old_including_files: &mut HashSet<PathBuf>, parent_shaders: &HashSet<PathBuf>, content: &str, pack_path: &Rc<PathBuf>,
        file_path: &Path, depth: i32,
    ) -> Vec<IncludeInformation> {
        let mut including_files = vec![];

        content
            .split_terminator('\n')
            .enumerate()
            .filter_map(|(line, content)| RE_MACRO_INCLUDE.captures(content).map(|captures| (line, content, captures)))
            .for_each(|(line, content, captures)| {
                let include_content = captures.get(1).unwrap();
                let path = include_content.as_str();
                match include_path_join(pack_path, file_path, path) {
                    Ok(include_path) => {
                        let already_includes = old_including_files.remove(&include_path);

                        if let Some(workspace_file) = workspace_files.get(&include_path) {
                            // File exists in workspace_files. If this is already included before modification, no need to update its includes.
                            if !already_includes {
                                workspace_file.extend_shader_list(workspace_files, parent_shaders, depth);
                                workspace_file.included_files.borrow_mut().insert(file_path.to_path_buf());
                            }
                        } else if let Some((temp_path, temp_file)) = temp_files.remove_entry(&include_path) {
                            temp_file.into_workspace_file(
                                workspace_files,
                                temp_files,
                                parser,
                                parent_shaders,
                                pack_path,
                                (&include_path, temp_path),
                                file_path,
                                depth,
                            );
                        } else {
                            Self::new_include(
                                workspace_files,
                                temp_files,
                                parser,
                                parent_shaders,
                                pack_path,
                                &include_path,
                                file_path,
                                depth,
                            );
                        }
                        let start_byte = include_content.start();
                        let start = unsafe { content.get_unchecked(..start_byte) }.chars().count();
                        let end = start + path.chars().count();
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
                including_file.update_shader_list(workspace_files, parent_shaders, depth);
            });
        including_files
    }

    pub fn new_shader(
        workspace_files: &mut HashMap<PathBuf, WorkspaceFile>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        pack_path: &Rc<PathBuf>, file_path: &Path,
    ) {
        let file_type = match file_path.extension() {
            Some(ext) if ext == "vsh" => gl::VERTEX_SHADER,
            Some(ext) if ext == "gsh" => gl::GEOMETRY_SHADER,
            Some(ext) if ext == "fsh" => gl::FRAGMENT_SHADER,
            Some(ext) if ext == "csh" => gl::COMPUTE_SHADER,
            // This will never be used since we have ensured the extension through basic shaders regex.
            _ => gl::NONE,
        };
        let workspace_file = if let Some(workspace_file) = workspace_files.get(file_path) {
            // Existing as some file's include
            let mut existing_file_type = workspace_file.file_type.borrow_mut();
            let scanned = *existing_file_type != gl::INVALID_ENUM;
            *existing_file_type = file_type;

            let mut parent_shader_list = workspace_file.parent_shaders.borrow_mut();
            parent_shader_list.insert(file_path.to_path_buf());
            // File already scanned. Just change its type to shaders.
            if scanned {
                return;
            }
            workspace_file.update_from_disc(parser, file_path);
            workspace_file
        } else {
            let parent_shaders = HashSet::from([file_path.to_path_buf()]);
            let shader_file = WorkspaceFile {
                file_type: RefCell::new(file_type),
                pack_path: pack_path.clone(),
                content: RefCell::new(String::new()),
                tree: RefCell::new(parser.parse("", None).unwrap()),
                line_mapping: RefCell::new(vec![]),
                included_files: RefCell::new(HashSet::new()),
                including_files: RefCell::new(vec![]),
                parent_shaders: RefCell::new(parent_shaders),
                diagnostics: RefCell::new(HashMap::new()),
            };
            shader_file.update_from_disc(parser, file_path);
            // Insert the shader file into workspace file list and takes the place.
            // Recursions in after call will only modify its included_files.
            workspace_files.insert_unique_unchecked(file_path.to_path_buf(), shader_file).1
        };

        // Clone the content so they can be used alone.
        let content = workspace_file.content.borrow().clone();
        let parent_shaders = workspace_file.parent_shaders.borrow().clone();

        let including_files = Self::update_include(
            workspace_files,
            temp_files,
            parser,
            &mut HashSet::new(),
            &parent_shaders,
            &content,
            pack_path,
            file_path,
            1,
        );
        *workspace_files.get(file_path).unwrap().including_files.borrow_mut() = including_files;
    }

    pub fn new_include(
        workspace_files: &mut HashMap<PathBuf, WorkspaceFile>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        parent_shaders: &HashSet<PathBuf>, pack_path: &Rc<PathBuf>, file_path: &Path, parent_path: &Path, depth: i32,
    ) {
        let include_file = WorkspaceFile {
            file_type: RefCell::new(gl::NONE),
            pack_path: pack_path.clone(),
            content: RefCell::new(String::new()),
            tree: RefCell::new(parser.parse("", None).unwrap()),
            line_mapping: RefCell::new(vec![]),
            included_files: RefCell::new(HashSet::from([parent_path.to_path_buf()])),
            including_files: RefCell::new(vec![]),
            parent_shaders: RefCell::new(parent_shaders.clone()),
            diagnostics: RefCell::new(HashMap::new()),
        };
        // Safety: the only call of new_include() already make sure that workspace_files does not contain file_path.
        let include_file = workspace_files.insert_unique_unchecked(file_path.to_path_buf(), include_file).1;
        if include_file.update_from_disc(parser, file_path) {
            // Clone the content so they can be used alone.
            if depth < 10 {
                let content = include_file.content.borrow().clone();
                let including_files = Self::update_include(
                    workspace_files,
                    temp_files,
                    parser,
                    &mut HashSet::new(),
                    parent_shaders,
                    &content,
                    pack_path,
                    file_path,
                    depth + 1,
                );
                *workspace_files.get(file_path).unwrap().including_files.borrow_mut() = including_files;
            }
        } else {
            *include_file.file_type.borrow_mut() = gl::INVALID_ENUM;
            error!("Include file {} not found in workspace!", file_path.to_str().unwrap());
        }
    }

    pub fn merge_file(
        &self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, file_list: &mut HashMap<PathBuf, String>, shader_content: &mut String,
        file_path: &Path, file_id: &mut i32, mut depth: u8,
    ) {
        *file_id += 1;
        let curr_file_id = file_list
            .entry(file_path.to_path_buf())
            .or_insert(Buffer::new().format(*file_id).to_owned())
            .clone();
        let file_name = file_path.to_str().unwrap();
        push_line_macro(shader_content, 1, &curr_file_id, file_name);
        shader_content.push('\n');

        let content = self.content.borrow();
        let mut start_index = 0;

        if depth < 10 {
            depth += 1;
            let line_mapping = self.line_mapping.borrow();
            let including_files = self.including_files.borrow();
            including_files
                .iter()
                .filter_map(|(line, _, _, include_path)| {
                    workspace_files
                        .get(include_path)
                        .map(|include_file| (line, include_path, include_file))
                })
                .filter(|(_, _, include_file)| *include_file.file_type.borrow() != gl::INVALID_ENUM)
                .for_each(|(line, include_path, include_file)| {
                    let start = line_mapping.get(*line).unwrap();
                    let end = line_mapping.get(line + 1).unwrap();

                    let before_content = unsafe { content.get_unchecked(start_index..*start) };
                    push_str_without_line(shader_content, before_content);
                    start_index = end - 1;

                    include_file.merge_file(workspace_files, file_list, shader_content, include_path, file_id, depth);
                    push_line_macro(shader_content, line + 2, &curr_file_id, file_name);
                });
        }
        push_str_without_line(shader_content, unsafe { content.get_unchecked(start_index..) });
        shader_content.push('\n');
    }

    pub fn clear(&self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, parser: &mut Parser, file_path: &Path) {
        *self.file_type.borrow_mut() = gl::INVALID_ENUM;
        self.content.borrow_mut().clear();
        *self.tree.borrow_mut() = parser.parse("", None).unwrap();
        self.line_mapping.borrow_mut().clear();
        self.diagnostics.borrow_mut().clear();

        let parent_shaders = self.parent_shaders.borrow();
        self.including_files
            .take()
            .into_iter()
            .map(|(_, _, _, include_path)| include_path)
            .collect::<HashSet<_>>()
            .iter()
            .filter_map(|include_path| {
                workspace_files
                    .get(include_path)
                    .map(|workspace_file| (include_path, workspace_file))
            })
            .for_each(|(_, workspace_file)| {
                workspace_file.included_files.borrow_mut().remove(file_path);
                workspace_file.update_shader_list(workspace_files, &parent_shaders, 0);
            });
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

    fn content(&self) -> &RefCell<String> {
        &self.content
    }

    fn tree(&self) -> &RefCell<Tree> {
        &self.tree
    }

    fn line_mapping(&self) -> &RefCell<Vec<usize>> {
        &self.line_mapping
    }

    fn including_files(&self) -> &RefCell<Vec<IncludeInformation>> {
        &self.including_files
    }
}
