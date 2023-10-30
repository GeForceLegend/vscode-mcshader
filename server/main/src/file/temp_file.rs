use logging::warn;

use super::*;

impl TempFile {
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
        if file_type != gl::INVALID_ENUM {
            for component in buffer {
                resource.push(component);
                match component {
                    Component::Prefix(_) | Component::RootDir => {}
                    _ => resource.push(MAIN_SEPARATOR_STR),
                }
            }
            resource.push("shaders");
        }

        let tree = parser.parse(&content, None).unwrap();
        let line_mapping = generate_line_mapping(&content);

        let temp_file = TempFile {
            file_type: RefCell::new(file_type),
            pack_path: Rc::new(PathBuf::from(resource)),
            content: RefCell::new(content),
            tree: RefCell::new(tree),
            line_mapping: RefCell::new(line_mapping),
            including_files: RefCell::new(vec![]),
        };

        temp_file.parse_includes(file_path);

        temp_file
    }

    pub fn parse_includes(&self, file_path: &Path) {
        if *self.file_type.borrow() == gl::INVALID_ENUM {
            return;
        }
        let pack_path = &self.pack_path;
        let mut including_files = self.including_files.borrow_mut();
        including_files.clear();

        self.content
            .borrow()
            .split_terminator('\n')
            .enumerate()
            .filter_map(|(line, content)| RE_MACRO_INCLUDE_TEMP.captures(content).map(|captures| (line, captures)))
            .for_each(|(line, captures)| {
                let include_content = captures.get(2).unwrap();
                let path = include_content.as_str();

                let line_content = captures.get(0).unwrap().as_str();
                let start_byte = include_content.start();
                let end_byte = include_content.end();
                let start = unsafe { line_content.get_unchecked(..start_byte) }.chars().count();
                let end = start + unsafe { line_content.get_unchecked(start_byte..end_byte) }.chars().count();

                match captures.get(1).unwrap().as_str() {
                    "include" => match include_path_join(pack_path, file_path, path) {
                        Ok(include_path) => including_files.push((line, start, end, Rc::new(include_path))),
                        Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
                    },
                    _ => {
                        // If marco name is not include, it must be moj_import
                        let additional_path = "include".to_owned() + MAIN_SEPARATOR_STR + path;
                        let include_path = pack_path.join(additional_path);

                        including_files.push((line, start, end, Rc::new(include_path)));
                    }
                }
            });
    }

    pub fn merge_self(&self, file_path: &Path) -> Option<String> {
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
        let including_files = self.including_files.borrow();
        let mut start_index = 0;

        for (line, _start, _end, include_path) in including_files.iter() {
            let start = line_mapping.get(*line).unwrap();
            let end = line_mapping.get(line + 1).unwrap();

            let before_content = unsafe { content.get_unchecked(start_index..*start) };
            push_str_without_line(&mut temp_content, before_content);
            start_index = *end - 1;

            if Self::merge_temp(&self.pack_path, include_path, &mut temp_content, &mut file_id, 1) {
                push_line_macro(&mut temp_content, line + 2, "0", file_name);
            } else {
                temp_content.push_str(unsafe { content.get_unchecked(*start..start_index) });
            }
        }
        push_str_without_line(&mut temp_content, unsafe { content.get_unchecked(start_index..) });

        Some(temp_content)
    }

    fn merge_temp(pack_path: &Path, file_path: &Path, temp_content: &mut String, file_id: &mut i32, depth: i32) -> bool {
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

            RE_MACRO_CATCH.find_iter(content.as_ref()).for_each(|macro_line| {
                let start = macro_line.start();
                let end = macro_line.end();

                let before_content = unsafe { content.get_unchecked(start_index..start) };
                let capture_content = macro_line.as_str();
                if let Some(capture) = RE_MACRO_INCLUDE.captures(capture_content) {
                    let path = capture.get(1).unwrap().as_str();

                    let include_path = match path.strip_prefix('/') {
                        Some(path) => pack_path.join(PathBuf::from(path.replace('/', MAIN_SEPARATOR_STR))),
                        None => file_path
                            .parent()
                            .unwrap()
                            .join(PathBuf::from(path.replace('/', MAIN_SEPARATOR_STR))),
                    };
                    temp_content.push_str(before_content);
                    start_index = end;
                    lines += before_content.matches('\n').count();

                    if Self::merge_temp(pack_path, &include_path, temp_content, file_id, depth + 1) {
                        push_line_macro(temp_content, lines, curr_file_id, file_name);
                    } else {
                        temp_content.push_str(capture_content);
                    }
                } else if RE_MACRO_LINE.is_match(capture_content) {
                    temp_content.push_str(before_content);
                    start_index = end;
                    lines += before_content.matches('\n').count();
                }
            });
            temp_content.push_str(unsafe { content.get_unchecked(start_index..) });
            temp_content.push('\n');
            true
        } else {
            warn!("Unable to read file"; "path" => file_path.to_str().unwrap());
            false
        }
    }

    pub fn into_workspace_file(
        self, workspace_files: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>, temp_files: &mut HashMap<PathBuf, TempFile>,
        parser: &mut Parser, parent_shaders: &HashSet<Rc<PathBuf>>, pack_path: &Rc<PathBuf>, file_path: PathBuf,
        parent_path: &Rc<PathBuf>, parent_file: &Rc<WorkspaceFile>, depth: i32,
    ) -> (Rc<PathBuf>, Rc<WorkspaceFile>) {
        let workspace_file = Rc::new(WorkspaceFile {
            file_type: RefCell::new(gl::NONE),
            pack_path: pack_path.clone(),
            content: self.content,
            tree: self.tree,
            line_mapping: self.line_mapping,
            included_files: RefCell::new(HashMap::from([(parent_path.clone(), parent_file.clone())])),
            including_files: RefCell::new(vec![]),
            parent_shaders: RefCell::new(parent_shaders.clone()),
            diagnostics: RefCell::new(HashMap::new()),
        });
        let file_path = workspace_files
            .insert_unique_unchecked(Rc::new(file_path), workspace_file.clone())
            .0
            .clone();

        if depth < 10 {
            let including_files = WorkspaceFile::update_include(
                workspace_files,
                temp_files,
                parser,
                &workspace_file,
                &mut HashMap::new(),
                parent_shaders,
                &file_path,
                depth + 1,
            );
            *workspace_file.including_files.borrow_mut() = including_files;
        }
        (file_path, workspace_file)
    }
}

impl File for TempFile {
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
            .map(|(line, start, end, include_path)| {
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
