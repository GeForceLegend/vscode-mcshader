use super::*;

impl MinecraftLanguageServer {
    pub fn change_file(&self, url: Url, changes: &[TextDocumentContentChangeEvent]) -> Option<Diagnostics> {
        let file_path = url.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();
        let temp_lint = server_data.temp_lint.borrow();

        let diagnostics = if let Some((file_path, workspace_file)) = workspace_files.get_key_value(&file_path) {
            workspace_file.apply_edit(changes, &mut parser);
            // Clone the content so they can be used alone.
            let file_path = file_path.clone();
            let pack_path = workspace_file.pack_path().clone();
            let content = workspace_file.content().borrow().clone();
            let mut old_including_files = workspace_file.including_pathes();
            let parent_shaders = workspace_file.parent_shaders().borrow().clone();

            let new_including_files = WorkspaceFile::update_include(
                &mut workspace_files,
                &mut temp_files,
                &mut parser,
                &mut old_including_files,
                &parent_shaders,
                &content,
                &pack_path,
                &file_path,
                1,
            );
            *workspace_files.get(&file_path).unwrap().including_files().borrow_mut() = new_including_files;

            self.collect_diagnostics(&workspace_files, &old_including_files)
        } else {
            let temp_file = temp_files.get(&file_path)?;
            temp_file.apply_edit(changes, &mut parser);
            temp_file.parse_includes(&file_path);
            let file_type = *temp_file.file_type().borrow();
            if file_type == gl::INVALID_ENUM || file_type == gl::NONE {
                let diagnostics = if *temp_lint {
                    TreeParser::simple_lint(
                        &temp_file.tree().borrow(),
                        &temp_file.content().borrow(),
                        &temp_file.line_mapping().borrow(),
                    )
                } else {
                    vec![]
                };
                HashMap::from([(url, diagnostics)])
            } else {
                return None;
            }
        };

        self.collect_memory(&mut workspace_files);
        Some(diagnostics)
    }
}
