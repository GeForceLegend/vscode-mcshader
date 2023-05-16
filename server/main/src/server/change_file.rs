use super::*;

impl MinecraftLanguageServer {
    pub fn change_file(&self, url: Url, changes: Vec<TextDocumentContentChangeEvent>) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let file_path = url.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();
        let temp_lint = server_data.temp_lint.borrow();

        let diagnostics = if let Some(workspace_file) = workspace_files.get(&file_path) {
            workspace_file.apply_edit(changes, &mut parser);
            // Clone the content so they can be used alone.
            let pack_path = workspace_file.pack_path().clone();
            let content = workspace_file.content().borrow().clone();
            let old_including_files = workspace_file.including_pathes();

            WorkspaceFile::update_include(
                &mut workspace_files,
                &mut temp_files,
                &mut parser,
                old_including_files,
                &content,
                &pack_path,
                &file_path,
                0,
            );
            HashMap::new()
        } else if let Some(temp_file) = temp_files.get(&file_path) {
            temp_file.apply_edit(changes, &mut parser);
            temp_file.parse_includes(&file_path);
            let file_type = *temp_file.file_type().borrow();
            if file_type == gl::INVALID_ENUM || file_type == gl::NONE {
                if *temp_lint {
                    HashMap::from([(url, TreeParser::simple_lint(&temp_file.tree().borrow()))])
                } else {
                    HashMap::from([(url, vec![])])
                }
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        self.collect_memory(&mut workspace_files);
        Some(diagnostics)
    }
}
