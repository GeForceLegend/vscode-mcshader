use super::*;

impl MinecraftLanguageServer {
    pub fn open_file(&self, params: DidOpenTextDocumentParams) -> Option<Diagnostics> {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let workspace_files = server_data.workspace_files.borrow();
        let mut temp_files = server_data.temp_files.borrow_mut();
        let temp_lint = server_data.temp_lint.borrow();

        if workspace_files.contains_key(&file_path) {
            None
        } else {
            let temp_file = TempFile::new(&mut parser, &file_path, params.text_document.text);
            let diagnostics = self.lint_temp_file(&temp_file, &file_path, params.text_document.uri, *temp_lint);
            temp_files.insert(file_path, temp_file);
            Some(diagnostics)
        }
    }
}
