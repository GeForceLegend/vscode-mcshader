use super::*;

impl MinecraftLanguageServer {
    pub fn open_file(&self, params: DidOpenTextDocumentParams) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let workspace_files = server_data.workspace_files.borrow();
        let mut temp_files = server_data.temp_files.borrow_mut();
        let temp_lint = server_data.temp_lint.borrow();

        let diagnostics = if let Some(workspace_file) = workspace_files.get(&file_path) {
            let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
            let mut shader_files = HashMap::new();
            workspace_file.get_base_shaders(&workspace_files, &mut shader_files, &file_path, 0);

            for (shader_path, shader_file) in shader_files {
                self.lint_workspace_shader(&workspace_files, shader_file, shader_path, &mut diagnostics);
            }
            diagnostics
        } else {
            let temp_file = TempFile::new(&mut parser, &file_path, params.text_document.text);
            let diagnostics = self.lint_temp_file(&temp_file, &file_path, params.text_document.uri, *temp_lint);
            temp_files.insert(file_path, temp_file);
            diagnostics
        };
        // self.update_diagnostics(&workspace_files, &temp_files, &diagnostics);
        Some(diagnostics)
    }
}
