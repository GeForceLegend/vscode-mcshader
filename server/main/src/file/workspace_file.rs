use super::*;

impl WorkspaceFile {
    pub fn included_files(&self) -> &RefCell<HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>> {
        &self.included_files
    }

    pub fn parent_shaders(&self) -> &RefCell<HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>> {
        &self.parent_shaders
    }

    pub fn diagnostics(&self) -> &RefCell<HashMap<Rc<PathBuf>, Vec<Diagnostic>>> {
        &self.diagnostics
    }

    pub fn including_files(&self) -> &RefCell<Vec<IncludeInformation>> {
        &self.including_files
    }

    pub fn new(parser: &mut Parser, file_type: u32, pack_path: &Rc<PathBuf>) -> Self {
        Self {
            file_type: RefCell::new(file_type),
            pack_path: pack_path.clone(),
            content: RefCell::new(String::new()),
            tree: RefCell::new(parser.parse("", None).unwrap()),
            line_mapping: RefCell::new(vec![]),
            included_files: RefCell::new(HashMap::new()),
            including_files: RefCell::new(vec![]),
            parent_shaders: RefCell::new(HashMap::new()),
            diagnostics: RefCell::new(HashMap::new()),
        }
    }

    fn extend_shader_list(
        &self, parent_shaders: &HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, mut depth: i32,
    ) {
        self.parent_shaders.borrow_mut().extend(parent_shaders.iter().map(|(path, file)| (path.clone(), file.clone())));

        if depth < 10 {
            depth += 1;
            self.including_files
                .borrow()
                .iter()
                .map(|(_, _, _, including_path, include_file)| (including_path, include_file))
                .collect::<HashMap<_, _>>()
                .into_iter()
                .for_each(|(_, including_file)| including_file.extend_shader_list(parent_shaders, depth));
        }
    }

    fn update_shader_list(
        &self, may_removed: &HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, mut depth: i32,
    ) {
        {
            let mut new_parent_shaders = HashMap::new();
            self.included_files
                .borrow()
                .iter()
                .for_each(|(_, workspace_file)| new_parent_shaders.extend(workspace_file.parent_shaders.borrow().iter().map(|(path, file)| (path.clone(), file.clone()))));

            let mut diagnostics = self.diagnostics.borrow_mut();
            may_removed.iter().filter(|(path, _)| !new_parent_shaders.contains_key(*path)).for_each(|(deleted_path, _)| {
                diagnostics.remove(deleted_path);
            });
            *self.parent_shaders.borrow_mut() = new_parent_shaders;
        }

        if depth < 10 {
            depth += 1;
            self.including_files
                .borrow()
                .iter()
                .map(|(_, _, _, including_path, include_file)| (including_path, include_file))
                .collect::<HashMap<_, _>>()
                .into_iter()
                .for_each(|(_, including_file)| including_file.update_shader_list(may_removed, depth));
        }
    }

    /// Sending the standalone clone data of a shader file to update its include.
    /// `parent_shaders` may get modified in child trees, so it should be cloned.
    /// Or it might get a borrow_mut() call while its already immutable borrowed.
    pub fn update_include(
        workspace_files: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        workspace_file: &Rc<WorkspaceFile>, old_including_files: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, parent_shaders: &HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>,
        file_path: &Rc<PathBuf>, depth: i32,
    ) {
        let mut including_files = vec![];

        let pack_path = workspace_file.pack_path();
        let content = workspace_file.content().borrow();

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

                        let (include_path, include_file) = if let Some((include_path, workspace_file)) = workspace_files.get_key_value(&include_path) {
                            // File exists in workspace_files. If this is already included before modification, no need to update its includes.
                            if already_includes.is_none() {
                                workspace_file.extend_shader_list(parent_shaders, depth);
                                workspace_file.included_files.borrow_mut().insert(file_path.clone(), workspace_file.clone());
                            }
                            (include_path.clone(), workspace_file.clone())
                        } else if let Some(temp_file) = temp_files.remove(&include_path) {
                            temp_file.into_workspace_file(
                                workspace_files,
                                temp_files,
                                parser,
                                parent_shaders,
                                pack_path,
                                include_path,
                                file_path,
                                workspace_file,
                                depth,
                            )
                        } else {
                            Self::new_include(
                                workspace_files,
                                temp_files,
                                parser,
                                parent_shaders,
                                pack_path,
                                include_path,
                                file_path,
                                workspace_file,
                                depth,
                            )
                        };
                        let start_byte = include_content.start();
                        let start = unsafe { content.get_unchecked(..start_byte) }.chars().count();
                        let end = start + path.chars().count();
                        including_files.push((line, start, end, include_path, include_file));
                    }
                    Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
                }
            });
        // They are removed from including list of this file. Let's remove this file from their parent list.
        old_including_files
            .iter()
            .for_each(|(_, including_file)| {
                including_file.included_files.borrow_mut().remove(file_path);
                including_file.update_shader_list(parent_shaders, depth);
            });
        *workspace_file.including_files.borrow_mut() = including_files;
    }

    pub fn new_shader(
        workspace_files: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        pack_path: &Rc<PathBuf>, file_path: PathBuf,
    ) {
        let file_type = match file_path.extension() {
            Some(ext) if ext == "vsh" => gl::VERTEX_SHADER,
            Some(ext) if ext == "gsh" => gl::GEOMETRY_SHADER,
            Some(ext) if ext == "fsh" => gl::FRAGMENT_SHADER,
            Some(ext) if ext == "csh" => gl::COMPUTE_SHADER,
            // This will never be used since we have ensured the extension through basic shaders regex.
            _ => gl::NONE,
        };
        let (file_path, parent_shaders, workspace_file) =
            if let Some((file_path, workspace_file)) = workspace_files.get_key_value(&file_path) {
                // Existing as some file's include
                let mut existing_file_type = workspace_file.file_type.borrow_mut();
                let scanned = *existing_file_type != gl::INVALID_ENUM;
                *existing_file_type = file_type;

                let mut parent_shader_list = workspace_file.parent_shaders.borrow_mut();
                parent_shader_list.insert(file_path.clone(), workspace_file.clone());
                // File already scanned. Just change its type to shaders.
                if scanned {
                    return;
                }
                workspace_file.update_from_disc(parser, file_path);
                (file_path.clone(), parent_shader_list.clone(), workspace_file)
            } else {
                let shader_path = Rc::new(file_path);
                let shader_file = Rc::new(WorkspaceFile {
                    file_type: RefCell::new(file_type),
                    pack_path: pack_path.clone(),
                    content: RefCell::new(String::new()),
                    tree: RefCell::new(parser.parse("", None).unwrap()),
                    line_mapping: RefCell::new(vec![]),
                    included_files: RefCell::new(HashMap::new()),
                    including_files: RefCell::new(vec![]),
                    parent_shaders: RefCell::new(HashMap::new()),
                    diagnostics: RefCell::new(HashMap::new()),
                });
                let parent_shaders = HashMap::from([(shader_path.clone(), shader_file.clone())]);
                *shader_file.parent_shaders.borrow_mut() = parent_shaders.clone();
                shader_file.update_from_disc(parser, &shader_path);
                // Insert the shader file into workspace file list and takes the place.
                // Recursions in after call will only modify its included_files.
                let (file_path, workspace_file) = workspace_files.insert_unique_unchecked(shader_path, shader_file);
                (file_path.clone(), parent_shaders, workspace_file as &Rc<WorkspaceFile>)
            };

        let workspace_file = workspace_file.clone();
        Self::update_include(
            workspace_files,
            temp_files,
            parser,
            &workspace_file,
            &mut HashMap::new(),
            &parent_shaders,
            &file_path,
            1,
        );
    }

    pub fn new_include(
        workspace_files: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        parent_shaders: &HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, pack_path: &Rc<PathBuf>, file_path: PathBuf, parent_path: &Rc<PathBuf>,
        parent_file: &Rc<WorkspaceFile>, depth: i32,
    ) -> (Rc<PathBuf>, Rc<WorkspaceFile>) {
        let include_file = WorkspaceFile {
            file_type: RefCell::new(gl::NONE),
            pack_path: pack_path.clone(),
            content: RefCell::new(String::new()),
            tree: RefCell::new(parser.parse("", None).unwrap()),
            line_mapping: RefCell::new(vec![]),
            included_files: RefCell::new(HashMap::from([(parent_path.clone(), parent_file.clone())])),
            including_files: RefCell::new(vec![]),
            parent_shaders: RefCell::new(parent_shaders.clone()),
            diagnostics: RefCell::new(HashMap::new()),
        };
        // Safety: the only call of new_include() already make sure that workspace_files does not contain file_path.
        let (file_path, include_file) = workspace_files.insert_unique_unchecked(Rc::new(file_path), Rc::new(include_file));
        let file_path = file_path.clone();
        let include_file = include_file.clone();
        if include_file.update_from_disc(parser, &file_path) && depth < 10 {
            // Clone the content so they can be used alone.
            Self::update_include(
                workspace_files,
                temp_files,
                parser,
                &include_file,
                &mut HashMap::new(),
                parent_shaders,
                &file_path,
                depth + 1,
            );
        } else {
            *include_file.file_type.borrow_mut() = gl::INVALID_ENUM;
            error!("Include file {} not found in workspace!", file_path.to_str().unwrap());
        }
        (file_path, include_file)
    }

    pub fn merge_file(
        &self, file_list: &mut HashMap<Rc<PathBuf>, (String, Rc<WorkspaceFile>)>, rc_self: &Rc<WorkspaceFile>, shader_content: &mut String, file_path: &Rc<PathBuf>, file_id: &mut i32, mut depth: u8,
    ) {
        *file_id += 1;
        let curr_file_id = file_list
            .entry(file_path.clone())
            .or_insert((Buffer::new().format(*file_id).to_owned(), rc_self.clone())).0
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
                .filter(|(_, _, _, _, include_file)| *include_file.file_type.borrow() != gl::INVALID_ENUM)
                .for_each(|(line, _, _, include_path, include_file)| {
                    let start = line_mapping.get(*line).unwrap();
                    let end = line_mapping.get(line + 1).unwrap();

                    let before_content = unsafe { content.get_unchecked(start_index..*start) };
                    push_str_without_line(shader_content, before_content);
                    start_index = end - 1;

                    include_file.merge_file(file_list, include_file, shader_content, include_path, file_id, depth);
                    push_line_macro(shader_content, line + 2, &curr_file_id, file_name);
                });
        }
        push_str_without_line(shader_content, unsafe { content.get_unchecked(start_index..) });
        shader_content.push('\n');
    }

    pub fn clear(&self, parser: &mut Parser, file_path: &PathBuf) {
        *self.file_type.borrow_mut() = gl::INVALID_ENUM;
        self.content.borrow_mut().clear();
        *self.tree.borrow_mut() = parser.parse("", None).unwrap();
        self.line_mapping.borrow_mut().clear();
        self.diagnostics.borrow_mut().clear();

        let parent_shaders = self.parent_shaders.borrow();
        self.including_files
            .take()
            .into_iter()
            .map(|(_, _, _, include_path, include_file)| (include_path, include_file))
            .collect::<HashMap<_, _>>()
            .iter()
            .for_each(|(_, workspace_file)| {
                workspace_file.included_files.borrow_mut().remove(file_path);
                workspace_file.update_shader_list(&parent_shaders, 0);
            });
    }

    pub fn including_pathes(&self) -> HashMap<Rc<PathBuf>, Rc<WorkspaceFile>> {
        self.including_files()
            .borrow()
            .iter()
            .map(|including_data| (including_data.3.clone(), including_data.4.clone()))
            .collect::<HashMap<_,_>>()
    }
}

impl File for WorkspaceFile {
    fn file_type(&self) -> &RefCell<u32> {
        &self.file_type
    }

    fn pack_path(&self) -> &Rc<PathBuf> {
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

    fn include_links(&self) -> Vec<DocumentLink> {
        self.including_files
            .borrow()
            .iter()
            .map(|(line, start, end, include_path, _)| {
                let url = Url::from_file_path(include_path as &Path).unwrap();
                DocumentLink {
                    range: Range {
                        start: Position {
                            line: *line as u32,
                            character: *start as u32,
                        },
                        end: Position {
                            line: *line as u32,
                            character: *end as u32,
                        },
                    },
                    tooltip: Some(include_path.to_str().unwrap().to_owned()),
                    target: Some(url),
                    data: None,
                }
            })
            .collect::<Vec<_>>()
    }
}
