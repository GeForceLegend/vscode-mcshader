use super::*;

impl TempFile {
    pub fn shader_pack(&self) -> &ShaderPack {
        &self.shader_pack
    }

    pub fn new(parser: &mut Parser, file_path: &Path, content: String) -> Self {
        warn!("Document not found in file system"; "path" => file_path.to_str().unwrap());
        let mut file_type = match file_path.extension() {
            Some(ext) if ext == "vsh" => gl::VERTEX_SHADER,
            Some(ext) if ext == "gsh" => gl::GEOMETRY_SHADER,
            Some(ext) if ext == "fsh" => gl::FRAGMENT_SHADER,
            Some(ext) if ext == "csh" => gl::COMPUTE_SHADER,
            _ => gl::NONE,
        };

        let mut buffer = file_path.components();
        loop {
            match buffer.next_back() {
                Some(Component::Normal(file_name)) => {
                    if file_name == "shaders" {
                        break;
                    }
                }
                _ => {
                    file_type = gl::INVALID_ENUM;
                    break;
                }
            }
        }

        let mut resource = OsString::new();
        let mut cache = None;
        if file_type != gl::INVALID_ENUM {
            for component in buffer {
                resource.push(component);
                match component {
                    Component::Prefix(_) | Component::RootDir => {}
                    _ => resource.push(MAIN_SEPARATOR_STR),
                }
            }
            resource.push("shaders");
            if file_type != gl::NONE {
                cache = Some(CompileCache::new());
            }
        }

        let tree = parser.parse(&content, None).unwrap();
        let line_mapping = generate_line_mapping(&content);
        let pack_path = PathBuf::from(resource);
        let debug = pack_path
            .parent()
            .and_then(|parent| parent.file_name())
            .map_or(false, |name| name == "debug");

        let temp_file = TempFile {
            file_type: RefCell::new(file_type),
            shader_pack: ShaderPack { path: pack_path, debug },
            content: RefCell::new(content),
            version: RefCell::new(None),
            cache: RefCell::new(cache),
            tree: RefCell::new(tree),
            line_mapping: RefCell::new(line_mapping),
            ignored_lines: RefCell::new(vec![]),
            including_files: RefCell::new(vec![]),
        };

        temp_file.parse_includes(file_path);

        temp_file
    }

    pub fn parse_includes(&self, file_path: &Path) {
        if *self.file_type.borrow() == gl::INVALID_ENUM {
            return;
        }
        let pack_path = &self.shader_pack.path;
        let mut including_files = self.including_files.borrow_mut();
        including_files.clear();
        let mut ignored_lines = vec![];
        let mut version = None;

        let content = self.content.borrow();
        let line_mapping = self.line_mapping.borrow();
        let mut start_index = 0;
        for i in 1..line_mapping.len() {
            let end_index = *line_mapping.get(i).unwrap();
            let content = &content[start_index..(end_index - 1)];
            let start_index_copy = start_index;
            start_index = end_index;
            let captures = match RE_MACRO_PARSER_TEMP.captures(content) {
                Some(captures) => captures,
                None => continue,
            };

            let line = i - 1;
            let capture_type = captures.get(1).unwrap();
            if capture_type.as_str() == "version" {
                if version.is_none() {
                    version = Some((start_index_copy, end_index - 1));
                }
                ignored_lines.push(line);
                continue;
            } else if capture_type.as_str() == "line" {
                ignored_lines.push(line);
                continue;
            }
            let include_content = captures.get(3).unwrap();
            let path = include_content.as_str();

            let line_content = captures.get(0).unwrap().as_str();
            let start_byte = include_content.start();
            let end_byte = include_content.end();
            let start = unsafe { line_content.get_unchecked(..start_byte) }.chars().count();
            let end = start + unsafe { line_content.get_unchecked(start_byte..end_byte) }.chars().count();

            match captures.get(2).unwrap().as_str() {
                "include" => match include_path_join(pack_path, file_path, path) {
                    Ok(include_path) => including_files.push((line, start, end, include_path)),
                    Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
                },
                _ => {
                    // If marco name is not include, it must be moj_import
                    let additional_path = "include".to_owned() + MAIN_SEPARATOR_STR + path;
                    let include_path = pack_path.join(additional_path);

                    including_files.push((line, start, end, include_path));
                }
            }
        }
        *self.version.borrow_mut() = version;
        *self.ignored_lines.borrow_mut() = ignored_lines;
    }

    pub fn merge_self(&self, file_path: &Path) -> Option<(String, String)> {
        let file_type = *self.file_type.borrow();
        if file_type == gl::NONE || file_type == gl::INVALID_ENUM {
            return None;
        }

        let mut temp_content = String::new();
        let mut file_id = 0;
        let file_name = file_path.to_str().unwrap();
        temp_content += "#line 1 0\t// ";
        temp_content += file_name;
        temp_content.push('\n');

        let content = self.content.borrow();
        let line_mapping = self.line_mapping.borrow();
        let ignored_lines = self.ignored_lines.borrow();
        let mut ignored_lines = ignored_lines.iter();
        let including_files = self.including_files.borrow();
        let mut start_index = 0;

        let mut version = self.version.borrow().as_ref().map_or(String::new(), |(start, end)| unsafe {
            content.get_unchecked(*start..*end).to_owned()
        });

        for (line, _start, _end, include_path) in including_files.iter() {
            let start = line_mapping.get(*line).unwrap();
            let end = line_mapping.get(line + 1).unwrap();

            push_str_without_ignored(
                &mut temp_content,
                &content,
                start_index,
                *start,
                *line,
                &mut ignored_lines,
                &line_mapping,
            );
            start_index = *end - 1;

            if Self::merge_temp(
                &self.shader_pack.path,
                include_path,
                &mut temp_content,
                &mut version,
                &mut file_id,
                1,
            ) {
                push_line_macro(&mut temp_content, line + 2, "0", file_name);
            } else {
                temp_content.push_str(unsafe { content.get_unchecked(*start..start_index) });
            }
        }
        push_str_without_ignored(
            &mut temp_content,
            &content,
            start_index,
            content.len(),
            line_mapping.len(),
            &mut ignored_lines,
            &line_mapping,
        );

        Some((temp_content, version))
    }

    fn merge_temp(
        pack_path: &Path, file_path: &Path, temp_content: &mut String, version: &mut String, file_id: &mut i32, depth: i32,
    ) -> bool {
        if depth > 10 {
            return false;
        }
        if let Ok(content) = read_to_string(file_path) {
            *file_id += 1;
            let mut buffer = Buffer::new();
            let curr_file_id = buffer.format(*file_id);
            let file_name = file_path.to_str().unwrap();
            push_line_macro(temp_content, 1, curr_file_id, file_name);
            temp_content.push('\n');

            let mut start_index = 0;
            let mut lines = 2;

            RE_MACRO_PARSER_MULTI_LINE.captures_iter(content.as_ref()).for_each(|captures| {
                let line = captures.get(0).unwrap();
                let start = line.start();
                let end = line.end();

                let before_content = unsafe { content.get_unchecked(start_index..start) };
                temp_content.push_str(before_content);
                lines += before_content.matches('\n').count();
                start_index = end;

                let capture_type = captures.get(1).unwrap();
                if capture_type.as_str() == "version" {
                    if version.is_empty() {
                        *version = line.as_str().to_owned();
                    }
                    return;
                } else if capture_type.as_str() == "line" {
                    return;
                }
                let include_path = captures.get(3).unwrap().as_str();
                let include_path = match captures.get(2).unwrap().as_str() {
                    "include" => match include_path_join(pack_path, file_path, include_path) {
                        Ok(include_path) => include_path,
                        Err(error) => {
                            error!("Unable to parse include link {}, error: {}", include_path, error);
                            return;
                        }
                    },
                    // moj_import
                    _ => {
                        let additional_path = "include".to_owned() + MAIN_SEPARATOR_STR + include_path;
                        pack_path.join(additional_path)
                    }
                };
                if Self::merge_temp(pack_path, &include_path, temp_content, version, file_id, depth + 1) {
                    push_line_macro(temp_content, lines, curr_file_id, file_name);
                } else {
                    temp_content.push_str(line.as_str());
                }
            });
            temp_content.push_str(unsafe { content.get_unchecked(start_index..) });
            temp_content.push('\n');
            true
        } else {
            warn!("Unable to read temp file"; "path" => file_path.to_str().unwrap());
            false
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn into_workspace_file(
        self, workspace_files: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, temp_files: &mut HashMap<PathBuf, TempFile>,
        parser: &mut Parser, file_path: PathBuf, parent_path: &Rc<PathBuf>, parent_file: &Rc<WorkspaceFile>, depth: i32,
    ) -> (Rc<PathBuf>, Rc<WorkspaceFile>) {
        let workspace_file = Rc::new(WorkspaceFile {
            file_type: RefCell::new(gl::NONE),
            shader_pack: parent_file.shader_pack.clone(),
            content: self.content,
            version: RefCell::new(None),
            cache: RefCell::new(None),
            tree: self.tree,
            line_mapping: self.line_mapping,
            ignored_lines: self.ignored_lines,
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
        });
        let file_path = Rc::new(file_path);
        workspace_files.insert_unique_unchecked(file_path.clone(), workspace_file.clone());

        if depth < 10 {
            WorkspaceFile::parse_content(
                workspace_files,
                temp_files,
                parser,
                &mut HashMap::new(),
                &workspace_file,
                &file_path,
                depth + 1,
            );
        }
        (file_path, workspace_file)
    }
}

impl ShaderFile for TempFile {
    fn file_type(&self) -> &RefCell<u32> {
        &self.file_type
    }

    fn content(&self) -> &RefCell<String> {
        &self.content
    }

    fn cache(&self) -> &RefCell<Option<CompileCache>> {
        &self.cache
    }

    fn tree(&self) -> &RefCell<Tree> {
        &self.tree
    }

    fn line_mapping(&self) -> &RefCell<Vec<usize>> {
        &self.line_mapping
    }

    fn ignored_lines(&self) -> &RefCell<Vec<usize>> {
        &self.ignored_lines
    }

    fn include_links(&self) -> Vec<DocumentLink> {
        self.including_files
            .borrow()
            .iter()
            .map(|(line, start, end, include_path)| {
                let url = Url::from_file_path(include_path).unwrap();
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
