use super::*;

impl MinecraftLanguageServer {
    pub fn list_symbols(&self, params: DocumentSymbolParams) -> Option<DocumentSymbolResponse> {
        let server_data = self.server_data.lock().unwrap();
        let workspace_files = server_data.workspace_files.borrow();
        let temp_files = server_data.temp_files.borrow();

        let file_path = params.text_document.uri.to_file_path().unwrap();

        let file: &dyn File = if let Some(workspace_file) = workspace_files.get(&file_path) {
            workspace_file
        } else if let Some(temp_file) = temp_files.get(&file_path) {
            temp_file
        } else {
            return None;
        };

        let content = file.content().borrow();
        let tree = file.tree().borrow();

        Some(DocumentSymbolResponse::Nested(TreeParser::list_symbols(&tree, &content)))
    }
}
