use super::*;

impl MinecraftLanguageServer {
    pub(super) fn collect_memory(&self, workspace_files: &mut HashMap<PathBuf, WorkspaceFile>) {
        workspace_files.retain(|_file_path, workspace_file| {
            // Only delete file that both do not exist and no file includes it.
            *workspace_file.file_type().borrow() != gl::INVALID_ENUM || workspace_file.included_files().borrow().len() > 0
        });
    }

    pub(super) fn scan_new_file(
        &self, parser: &mut Parser, shader_packs: &HashSet<PathBuf>, workspace_files: &mut HashMap<PathBuf, WorkspaceFile>,
        temp_files: &mut HashMap<PathBuf, TempFile>, file_path: &PathBuf,
    ) -> bool {
        for shader_pack in shader_packs {
            match file_path.strip_prefix(shader_pack) {
                Ok(relative_path) => {
                    let relative_path = relative_path.to_str().unwrap();
                    if RE_BASIC_SHADER.is_match(relative_path) {
                        WorkspaceFile::new_shader(workspace_files, temp_files, parser, &shader_pack, &file_path);
                        return true;
                    } else if let Some(result) = relative_path.split_once(MAIN_SEPARATOR_STR) {
                        if RE_DIMENSION_FOLDER.is_match(result.0) && RE_BASIC_SHADER.is_match(result.1) {
                            WorkspaceFile::new_shader(workspace_files, temp_files, parser, &shader_pack, &file_path);
                            return true;
                        }
                    }
                    return false;
                }
                Err(_) => continue,
            }
        }
        false
    }

    pub(super) fn find_shader_packs(&self, shader_packs: &mut Vec<PathBuf>, curr_path: &PathBuf) {
        curr_path.read_dir().unwrap().filter_map(|file| file.ok()).for_each(|file| {
            let file_path = file.path();
            if file_path.is_dir() {
                if file.file_name() == "shaders" {
                    info!("Find shader pack {}", file_path.to_str().unwrap());
                    shader_packs.push(file_path);
                } else {
                    self.find_shader_packs(shader_packs, &file_path);
                }
            }
        })
    }

    pub(super) fn scan_files_in_root(
        &self, parser: &mut Parser, shader_packs: &mut HashSet<PathBuf>, workspace_files: &mut HashMap<PathBuf, WorkspaceFile>,
        temp_files: &mut HashMap<PathBuf, TempFile>, root: PathBuf,
    ) {
        info!("Generating file framework on current root"; "root" => root.to_str().unwrap());

        let mut sub_shader_packs: Vec<PathBuf> = vec![];
        if root.file_name().unwrap() == "shaders" {
            sub_shader_packs.push(root);
        } else {
            self.find_shader_packs(&mut sub_shader_packs, &root);
        }

        for pack_path in &sub_shader_packs {
            pack_path.read_dir().unwrap().filter_map(|file| file.ok()).for_each(|file| {
                let file_path = file.path();
                if file_path.is_file() {
                    if RE_BASIC_SHADER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
                        WorkspaceFile::new_shader(workspace_files, temp_files, parser, pack_path, &file_path);
                    }
                } else if RE_DIMENSION_FOLDER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
                    file_path.read_dir().unwrap().filter_map(|file| file.ok()).for_each(|dim_file| {
                        let file_path = dim_file.path();
                        if file_path.is_file() && RE_BASIC_SHADER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
                            WorkspaceFile::new_shader(workspace_files, temp_files, parser, pack_path, &file_path);
                        }
                    })
                }
            })
        }

        shader_packs.extend(sub_shader_packs);
    }

    pub(super) fn lint_shader(
        &self, file_path: &PathBuf, file_type: u32, source: String, file_list: HashMap<String, Url>,
        diagnostics: &mut HashMap<Url, Vec<Diagnostic>>,
    ) {
        let validation_result = OPENGL_CONTEXT.validate_shader(file_type, source);

        match validation_result {
            Some(compile_log) => {
                info!(
                    "Compilation errors reported; shader file: {},\nerrors: \"\n{}\"",
                    file_path.to_str().unwrap(),
                    compile_log
                );
                DIAGNOSTICS_PARSER.parse_diagnostics(compile_log, file_list, file_path, diagnostics);
            }
            None => {
                info!("Compilation reported no errors"; "shader file" => file_path.to_str().unwrap());
                for (_, url) in file_list {
                    if !diagnostics.contains_key(&url) {
                        diagnostics.insert(url, vec![]);
                    }
                }
            }
        };
    }

    pub(super) fn lint_workspace_shader(
        &self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, shader_file: &WorkspaceFile, file_path: &PathBuf,
        diagnostics: &mut HashMap<Url, Vec<Diagnostic>>,
    ) {
        let mut file_list: HashMap<String, Url> = HashMap::new();
        let mut shader_content = String::new();
        shader_file.merge_file(workspace_files, &mut file_list, &mut shader_content, file_path, &mut 0, 0);
        preprocess_shader(&mut shader_content, shader_file.pack_path());

        self.lint_shader(file_path, *shader_file.file_type().borrow(), shader_content, file_list, diagnostics)
    }

    pub(super) fn lint_temp_file(&self, temp_file: &TempFile, file_path: &PathBuf, url: Url) -> HashMap<Url, Vec<Diagnostic>> {
        let mut diagnostics = HashMap::new();
        if let Some(result) = temp_file.merge_self(file_path) {
            let file_list = HashMap::from([("0".to_owned(), url)]);
            self.lint_shader(file_path, result.0, result.1, file_list, &mut diagnostics);
        } else {
            diagnostics.insert(url, TreeParser::simple_lint(&temp_file.tree().borrow()));
        }
        diagnostics
    }

    pub(super) fn update_diagnostics(
        &self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, temp_files: &HashMap<PathBuf, TempFile>,
        diagnostics: &HashMap<Url, Vec<Diagnostic>>,
    ) {
        for (url, diagnostics) in diagnostics {
            let file_path = url.to_file_path().unwrap();
            if let Some(workspace_file) = workspace_files.get(&file_path) {
                *workspace_file.diagnostics().borrow_mut() = diagnostics.clone();
            } else if let Some(temp_file) = temp_files.get(&file_path) {
                *temp_file.diagnostics().borrow_mut() = diagnostics.clone();
            }
        }
    }

    pub fn initial_scan(&self, roots: Vec<PathBuf>) {
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
