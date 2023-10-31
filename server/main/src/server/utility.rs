use super::*;

impl MinecraftLanguageServer {
    pub(super) fn collect_memory(&self, workspace_files: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>) {
        workspace_files.retain(|_file_path, workspace_file| {
            // Only delete file that both do not exist and no file includes it.
            *workspace_file.file_type().borrow() != gl::INVALID_ENUM || workspace_file.included_files().borrow().len() > 0
        });
    }

    pub(super) fn is_valid_shader<'a>(&'a self, shader_packs: &'a HashSet<Rc<PathBuf>>, file_path: &Path) -> Option<&Rc<PathBuf>> {
        for shader_pack in shader_packs {
            if let Ok(relative_path) = file_path.strip_prefix(shader_pack as &Path) {
                let relative_path = relative_path.to_str().unwrap();
                if RE_BASIC_SHADERS.is_match(relative_path) {
                    return Some(shader_pack);
                } else if let Some(result) = relative_path.split_once(MAIN_SEPARATOR) {
                    if RE_DIMENSION_FOLDER.is_match(result.0) && RE_BASIC_SHADERS.is_match(result.1) {
                        return Some(shader_pack);
                    }
                }
                return None;
            }
        }
        None
    }

    pub(super) fn find_shader_packs(shader_packs: &mut Vec<Rc<PathBuf>>, curr_path: PathBuf) {
        let file_name = curr_path.file_name().unwrap();
        if file_name == "shaders" {
            info!("Find shader pack {}", curr_path.to_str().unwrap());
            shader_packs.push(Rc::new(curr_path));
        } else if file_name
            .to_str()
            .map_or(true, |name| !name.starts_with('.') || name == ".minecraft")
        {
            curr_path
                .read_dir()
                .unwrap()
                .filter_map(|file| file.ok())
                .filter(|file| file.file_type().unwrap().is_dir())
                .for_each(|file| {
                    Self::find_shader_packs(shader_packs, file.path());
                })
        }
    }

    pub(super) fn scan_files_in_root(
        &self, parser: &mut Parser, shader_packs: &mut HashSet<Rc<PathBuf>>, workspace_files: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>,
        temp_files: &mut HashMap<PathBuf, TempFile>, root: PathBuf,
    ) {
        info!("Generating file framework on workspace \"{}\"", root.to_str().unwrap());

        let mut sub_shader_packs: Vec<Rc<PathBuf>> = vec![];
        Self::find_shader_packs(&mut sub_shader_packs, root);

        for pack_path in &sub_shader_packs {
            pack_path.read_dir().unwrap().filter_map(|file| file.ok()).for_each(|file| {
                let file_path = file.path();
                if file.file_type().unwrap().is_file() {
                    if RE_BASIC_SHADERS.is_match(file.file_name().to_str().unwrap()) {
                        WorkspaceFile::new_shader(workspace_files, temp_files, parser, pack_path, file_path);
                    }
                } else if RE_DIMENSION_FOLDER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
                    file_path.read_dir().unwrap().filter_map(|file| file.ok()).for_each(|dim_file| {
                        let dim_file_path = dim_file.path();
                        if dim_file.file_type().unwrap().is_file() && RE_BASIC_SHADERS.is_match(dim_file.file_name().to_str().unwrap()) {
                            WorkspaceFile::new_shader(workspace_files, temp_files, parser, pack_path, dim_file_path);
                        }
                    })
                }
            })
        }

        shader_packs.extend(sub_shader_packs);
    }

    pub(super) fn lint_workspace_shader(
        &self, shader_file: &Rc<WorkspaceFile>, file_path: &Rc<PathBuf>, update_list: &mut HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>,
    ) {
        let mut file_list = HashMap::new();
        let mut shader_content = String::new();
        shader_file.merge_file(&mut file_list, shader_file, &mut shader_content, file_path, &mut -1, 0);
        let offset = preprocess_shader(&mut shader_content, shader_file.pack_path());

        let shader_path = file_path.to_str().unwrap();
        let validation_result = OPENGL_CONTEXT.validate_shader(*shader_file.file_type().borrow(), shader_content);

        match validation_result {
            Some(compile_log) => {
                info!(
                    "Compilation errors reported; shader file: {},\nerrors: \"\n{}\"",
                    shader_path, compile_log
                );

                // We have ensured files in file lists are unique, so each file.diagnostics will exist only once
                // And they are `Rc<_>`, so their pointer will never get changed. This should be safe.
                let mut diagnostic_pointers = file_list
                    .into_iter()
                    .map(|(path, (index, file))| {
                        let pointer = file
                            .diagnostics()
                            .borrow_mut()
                            .entry(file_path.clone())
                            .and_modify(|diagnostics| diagnostics.clear())
                            .or_default() as *mut Vec<Diagnostic>;
                        update_list.insert(path, file);
                        (index, pointer)
                    })
                    .collect::<HashMap<_, _>>();

                compile_log
                    .split_terminator('\n')
                    .filter_map(|log_line| DIAGNOSTICS_REGEX.captures(log_line))
                    .for_each(|captures| {
                        let mut msg = captures.name("output").unwrap().as_str().to_owned() + ", from file: ";
                        msg += shader_path;

                        let line = captures.name("linenum").map_or(0, |c| c.as_str().parse::<u32>().unwrap_or(0)) - offset;

                        let severity = captures.name("severity").map_or(DiagnosticSeverity::INFORMATION, |c| {
                            match c.as_str().to_lowercase().as_str() {
                                "error" => DiagnosticSeverity::ERROR,
                                "warning" => DiagnosticSeverity::WARNING,
                                _ => DiagnosticSeverity::INFORMATION,
                            }
                        });

                        let diagnostic = Diagnostic {
                            range: Range {
                                start: Position { line, character: 0 },
                                end: Position { line, character: u32::MAX },
                            },
                            severity: Some(severity),
                            source: Some("mcshader-glsl".to_owned()),
                            message: msg,
                            ..Default::default()
                        };

                        let index = captures.name("filepath").unwrap();
                        if let Some(diagnostics) = diagnostic_pointers.get_mut(index.as_str()) {
                            unsafe { diagnostics.as_mut().unwrap().push(diagnostic) };
                        }
                    });
            }
            None => {
                info!("Compilation reported no errors"; "shader file" => shader_path);
                file_list.into_iter().for_each(|(file_path, (_, workspace_file))| {
                    workspace_file.diagnostics().borrow_mut().insert(file_path.clone(), vec![]);
                    update_list.insert(file_path, workspace_file);
                });
            }
        };
    }

    pub(super) fn lint_temp_file(&self, temp_file: &TempFile, file_path: &Path, url: Url, temp_lint: bool) -> Diagnostics {
        let diagnostics = if let Some(mut source) = temp_file.merge_self(file_path) {
            let file_type = *temp_file.file_type().borrow();
            let offset = preprocess_shader(&mut source, temp_file.pack_path());
            let validation_result = OPENGL_CONTEXT.validate_shader(file_type, source);

            match validation_result {
                Some(compile_log) => {
                    info!(
                        "Compilation errors reported; shader file: {},\nerrors: \"\n{}\"",
                        file_path.to_str().unwrap(),
                        compile_log
                    );
                    let diagnostics = compile_log
                        .split_terminator('\n')
                        .filter_map(|log_line| DIAGNOSTICS_REGEX.captures(log_line))
                        .filter(|captures| captures.name("filepath").unwrap().as_str() == "0")
                        .map(|captures| {
                            let msg = captures.name("output").unwrap().as_str().to_owned();

                            let line = captures.name("linenum").map_or(0, |c| c.as_str().parse::<u32>().unwrap_or(0)) - offset;

                            let severity = captures.name("severity").map_or(DiagnosticSeverity::INFORMATION, |c| {
                                match c.as_str().to_lowercase().as_str() {
                                    "error" => DiagnosticSeverity::ERROR,
                                    "warning" => DiagnosticSeverity::WARNING,
                                    _ => DiagnosticSeverity::INFORMATION,
                                }
                            });

                            Diagnostic {
                                range: Range {
                                    start: Position { line, character: 0 },
                                    end: Position { line, character: u32::MAX },
                                },
                                severity: Some(severity),
                                source: Some("mcshader-glsl".to_owned()),
                                message: msg,
                                ..Default::default()
                            }
                        })
                        .collect::<Vec<_>>();

                    diagnostics
                }
                None => {
                    info!("Compilation reported no errors"; "shader file" => file_path.to_str().unwrap());
                    vec![]
                }
            }
        } else if temp_lint {
            TreeParser::simple_lint(
                &temp_file.tree().borrow(),
                &temp_file.content().borrow(),
                &temp_file.line_mapping().borrow(),
            )
        } else {
            vec![]
        };
        HashMap::from([(url, diagnostics)])
    }

    pub(super) fn collect_diagnostics(&self, update_list: &HashMap<Rc<PathBuf>, Rc<WorkspaceFile>>) -> Diagnostics {
        update_list
            .into_iter()
            .map(|(file_path, workspace_file)| {
                let file_url = Url::from_file_path(file_path as &Path).unwrap();
                let diagnostics = workspace_file
                    .diagnostics()
                    .borrow()
                    .values()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>();
                (file_url, diagnostics)
            })
            .collect()
    }

    pub(super) fn initial_scan(&self, roots: Vec<PathBuf>) {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut shader_packs = server_data.shader_packs.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        for root in roots {
            self.scan_files_in_root(&mut parser, &mut shader_packs, &mut workspace_files, &mut temp_files, root);
        }
    }
}
