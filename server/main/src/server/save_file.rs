use super::*;

impl MinecraftLanguageServer {
    pub fn save_file(&self, file_path: PathBuf) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        let diagnostics = if let Some(workspace_file) = workspace_files.get(&file_path) {
            // If this file is ended with watched extension, it should get updated through update_watched_files
            if !server_data
                .extensions
                .borrow()
                .contains(file_path.extension().unwrap().to_str().unwrap())
            {
                workspace_file.update_from_disc(&mut parser, &file_path);
                // Clone the content so they can be used alone.
                let pack_path = workspace_file.pack_path().clone();
                let content = workspace_file.content().borrow().clone();
                let old_including_files = workspace_file.including_pathes();

                // Get the new pointer of this file (it might changed if workspace file list get reallocated).
                // workspace_files will not get modded after this call so this should be safe
                let workspace_file = WorkspaceFile::update_include(
                    &mut workspace_files,
                    &mut temp_files,
                    &mut parser,
                    old_including_files,
                    &content,
                    &pack_path,
                    &file_path,
                    0,
                );

                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                let mut shader_files = HashMap::new();
                unsafe {
                    workspace_file
                        .as_ref()
                        .unwrap()
                        .get_base_shaders(&workspace_files, &mut shader_files, &file_path, 0)
                };

                for (shader_path, shader_file) in shader_files {
                    self.lint_workspace_shader(&workspace_files, shader_file, shader_path, &mut diagnostics);
                }
                diagnostics
            } else {
                return None;
            }
        } else if let Some(temp_file) = temp_files.get_mut(&file_path) {
            temp_file.update_from_disc(&mut parser, &file_path);
            temp_file.parse_includes(&file_path);
            self.lint_temp_file(temp_file, &file_path)
        } else {
            return None;
        };
        self.update_diagnostics(&workspace_files, &temp_files, &diagnostics);

        self.collect_memory(&mut workspace_files);
        Some(diagnostics)
    }
}
