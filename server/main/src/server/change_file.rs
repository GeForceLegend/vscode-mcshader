use super::*;

impl MinecraftLanguageServer {
    pub fn change_file(&self, url: Url, changes: Vec<TextDocumentContentChangeEvent>) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let file_path = url.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        let compile_diagnostics;
        let tree = if let Some(workspace_file) = workspace_files.get(&file_path) {
            workspace_file.apply_edit(changes, &mut parser);
            compile_diagnostics = workspace_file.diagnostics().borrow().clone();
            // Clone the content so they can be used alone.
            let pack_path = workspace_file.pack_path().clone();
            let content = workspace_file.content().borrow().clone();
            let old_including_files = workspace_file.including_pathes();

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
            unsafe { workspace_file.as_ref().unwrap().tree().borrow() }
        } else if let Some(temp_file) = temp_files.get(&file_path) {
            temp_file.apply_edit(changes, &mut parser);
            compile_diagnostics = temp_file.diagnostics().borrow().clone();
            temp_file.parse_includes(&file_path);
            temp_file.tree().borrow()
        } else {
            return None;
        };

        let mut diagnostics = TreeParser::simple_lint(&tree);
        diagnostics.extend(compile_diagnostics);
        let diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::from([(url, diagnostics)]);

        self.collect_memory(&mut workspace_files);
        Some(diagnostics)
    }
}
