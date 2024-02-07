use super::*;

impl WorkspaceFile {
    pub fn shader_pack(&self) -> &Rc<ShaderPack> {
        &self.shader_pack
    }

    pub fn included_files(&self) -> &RefCell<HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>> {
        &self.included_files
    }

    pub fn parent_shaders(&self) -> &RefCell<HashMap<Rc<PathBuf>, ShaderData>> {
        &self.parent_shaders
    }

    pub fn including_files(&self) -> &RefCell<Vec<IncludeInformation>> {
        &self.including_files
    }

    pub fn new(parser: &mut Parser, file_type: u32, pack_path: &Rc<ShaderPack>) -> Self {
        Self {
            file_type: RefCell::new(file_type),
            shader_pack: pack_path.clone(),
            content: RefCell::new(String::new()),
            tree: RefCell::new(parser.parse("", None).unwrap()),
            line_mapping: RefCell::new(vec![]),
            included_files: RefCell::new(HashMap::new()),
            including_files: RefCell::new(vec![]),
            parent_shaders: RefCell::new(HashMap::new()),
        }
    }

    fn extend_shader_list(&self, parent_shaders: &HashMap<Rc<PathBuf>, ShaderData>, mut depth: i32) {
        self.parent_shaders.borrow_mut().extend(
            parent_shaders
                .iter()
                .map(|(path, data)| (path.clone(), (data.0.clone(), RefCell::new(vec![])))),
        );

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

    fn update_shader_list(&self, update_list: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, mut depth: i32) {
        {
            let mut new_parent_shaders = HashMap::new();
            // If we do not take, self-include will copy all previous shader files.
            let mut old_parent_shader = self.parent_shaders().take();
            self.included_files.borrow().iter().for_each(|(_, workspace_file)| {
                workspace_file.parent_shaders.borrow().iter().for_each(|(path, data)| {
                    if !new_parent_shaders.contains_key(path) {
                        match old_parent_shader.remove_entry(path) {
                            Some((path, data)) => new_parent_shaders.insert_unique_unchecked(path, data),
                            None => new_parent_shaders.insert_unique_unchecked(path.clone(), (data.0.clone(), RefCell::new(vec![]))),
                        };
                    }
                })
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
                .for_each(|(path, including_file)| {
                    update_list.insert(path.clone(), including_file.clone());
                    including_file.update_shader_list(update_list, depth)
                });
        }
    }

    pub fn update_include(
        workspace_files: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        update_list: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, workspace_file: &Rc<WorkspaceFile>, file_path: &Rc<PathBuf>, depth: i32,
    ) {
        let mut old_including_files = workspace_file.including_pathes();
        let mut including_files = vec![];

        let pack_path = &workspace_file.shader_pack;
        let content = workspace_file.content().borrow();
        let line_mapping = workspace_file.line_mapping().borrow();

        let mut start_index = 0;
        for i in 1..line_mapping.len() {
            let end_index = *line_mapping.get(i).unwrap();
            let content = &content[start_index..(end_index - 1)];
            start_index = end_index;
            let captures = match RE_MACRO_INCLUDE.captures(content) {
                Some(captures) => captures,
                None => continue,
            };

            let line = i - 1;
            let include_content = captures.get(1).unwrap();
            let path = include_content.as_str();
            match include_path_join(&pack_path.path, file_path, path) {
                Ok(include_path) => {
                    let (include_path, include_file) = if let Some((include_path, include_file)) =
                        workspace_files.get_key_value(&include_path)
                    {
                        // File exists in workspace_files. If this is already included before modification, no need to update its includes.
                        // If a file does not exist in workspace_files, then it's impossible to exists in old_including_files too.
                        match old_including_files.remove_entry(include_path) {
                            Some(include) => include,
                            None => {
                                // Parent shader of self might get extended in previous include scan.
                                // And it might get changed if it includes it self in its include tree, so we should clone here.
                                let parent_shaders = workspace_file.parent_shaders.borrow().clone();
                                include_file.extend_shader_list(&parent_shaders, depth);
                                include_file
                                    .included_files
                                    .borrow_mut()
                                    .insert(file_path.clone(), workspace_file.clone());
                                (include_path.clone(), include_file.clone())
                            }
                        }
                    } else if let Some(temp_file) = temp_files.remove(&include_path) {
                        temp_file.into_workspace_file(workspace_files, temp_files, parser, include_path, file_path, workspace_file, depth)
                    } else {
                        Self::new_include(workspace_files, temp_files, parser, include_path, file_path, workspace_file, depth)
                    };
                    let start_byte = include_content.start();
                    let start = unsafe { content.get_unchecked(..start_byte) }.chars().count();
                    let end = start + path.chars().count();
                    including_files.push((line, start, end, include_path, include_file));
                }
                Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
            }
        }
        // They are removed from including list of this file. Let's remove this file from their parent list.
        old_including_files.into_iter().for_each(|(include_path, including_file)| {
            including_file.included_files.borrow_mut().remove(file_path);
            including_file.update_shader_list(update_list, depth);
            update_list.insert(include_path, including_file);
        });
        *workspace_file.including_files.borrow_mut() = including_files;
    }

    pub fn new_shader(
        workspace_files: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        pack_path: &Rc<ShaderPack>, file_path: PathBuf,
    ) {
        let file_type = match file_path.extension() {
            Some(ext) if ext == "vsh" => gl::VERTEX_SHADER,
            Some(ext) if ext == "gsh" => gl::GEOMETRY_SHADER,
            Some(ext) if ext == "fsh" => gl::FRAGMENT_SHADER,
            Some(ext) if ext == "csh" => gl::COMPUTE_SHADER,
            // This will never be used since we have ensured the extension through basic shaders regex.
            _ => gl::NONE,
        };
        let (file_path, workspace_file) = if let Some((file_path, workspace_file)) = workspace_files.get_key_value(&file_path) {
            // Existing as some file's include
            let mut existing_file_type = workspace_file.file_type.borrow_mut();
            let scanned = *existing_file_type != gl::INVALID_ENUM;
            *existing_file_type = file_type;

            // File already scanned. Just change its type to shaders.
            if scanned {
                let new_parent_shaders = HashMap::from([(file_path.clone(), (workspace_file.clone(), RefCell::new(vec![])))]);
                workspace_file.extend_shader_list(&new_parent_shaders, 1);
                return;
            }
            let mut parent_shader_list = workspace_file.parent_shaders.borrow_mut();
            parent_shader_list.insert(file_path.clone(), (workspace_file.clone(), RefCell::new(vec![])));

            workspace_file.update_from_disc(parser, file_path);
            (file_path.clone(), workspace_file)
        } else {
            let shader_path = Rc::new(file_path);
            let shader_file = Rc::new(WorkspaceFile {
                file_type: RefCell::new(file_type),
                shader_pack: pack_path.clone(),
                content: RefCell::new(String::new()),
                tree: RefCell::new(parser.parse("", None).unwrap()),
                line_mapping: RefCell::new(vec![]),
                included_files: RefCell::new(HashMap::new()),
                including_files: RefCell::new(vec![]),
                parent_shaders: RefCell::new(HashMap::new()),
            });
            *shader_file.parent_shaders.borrow_mut() = HashMap::from([(shader_path.clone(), (shader_file.clone(), RefCell::new(vec![])))]);
            shader_file.update_from_disc(parser, &shader_path);
            // Insert the shader file into workspace file list and takes the place.
            // Recursions in after call will only modify its included_files.
            let (file_path, workspace_file) = workspace_files.insert_unique_unchecked(shader_path, shader_file);
            (file_path.clone(), workspace_file as &Rc<WorkspaceFile>)
        };

        let workspace_file = workspace_file.clone();
        Self::update_include(
            workspace_files,
            temp_files,
            parser,
            &mut HashMap::new(),
            &workspace_file,
            &file_path,
            1,
        );
    }

    pub fn new_include(
        workspace_files: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, temp_files: &mut HashMap<PathBuf, TempFile>, parser: &mut Parser,
        file_path: PathBuf, parent_path: &Rc<PathBuf>, parent_file: &Rc<WorkspaceFile>, depth: i32,
    ) -> (Rc<PathBuf>, Rc<WorkspaceFile>) {
        let include_file = WorkspaceFile {
            file_type: RefCell::new(gl::NONE),
            shader_pack: parent_file.shader_pack.clone(),
            content: RefCell::new(String::new()),
            tree: RefCell::new(parser.parse("", None).unwrap()),
            line_mapping: RefCell::new(vec![]),
            included_files: RefCell::new(HashMap::from([(parent_path.clone(), parent_file.clone())])),
            including_files: RefCell::new(vec![]),
            parent_shaders: RefCell::new(
                parent_file
                    .parent_shaders
                    .borrow()
                    .iter()
                    .map(|(path, data)| (path.clone(), (data.0.clone(), RefCell::new(vec![]))))
                    .collect(),
            ),
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
                &mut HashMap::new(),
                &include_file,
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
        &self, file_list: &mut HashMap<Rc<PathBuf>, (String, Rc<WorkspaceFile>)>, rc_self: &Rc<WorkspaceFile>, shader_content: &mut String,
        file_path: &Rc<PathBuf>, file_id: &mut i32, mut depth: u8,
    ) {
        *file_id += 1;
        let curr_file_id = file_list
            .entry(file_path.clone())
            .or_insert((Buffer::new().format(*file_id).to_owned(), rc_self.clone()))
            .0
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

    pub fn clear(&self, parser: &mut Parser, file_path: &PathBuf, update_list: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>) {
        *self.file_type.borrow_mut() = gl::INVALID_ENUM;
        self.content.borrow_mut().clear();
        *self.tree.borrow_mut() = parser.parse("", None).unwrap();
        self.line_mapping.borrow_mut().clear();

        self.including_files
            .take()
            .into_iter()
            .map(|(_, _, _, include_path, include_file)| (include_path, include_file))
            .collect::<HashMap<_, _>>()
            .into_iter()
            .for_each(|(path, workspace_file)| {
                workspace_file.included_files.borrow_mut().remove(file_path);
                workspace_file.update_shader_list(update_list, 0);
                update_list.insert(path, workspace_file);
            });
    }

    pub fn including_pathes(&self) -> HashMap<Rc<PathBuf>, Rc<WorkspaceFile>> {
        self.including_files()
            .borrow()
            .iter()
            .map(|including_data| (including_data.3.clone(), including_data.4.clone()))
            .collect::<HashMap<_, _>>()
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
