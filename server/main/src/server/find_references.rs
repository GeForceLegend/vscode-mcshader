use super::*;

impl MinecraftLanguageServer {
    pub fn find_references(&self, params: ReferenceParams) -> Option<Vec<Location>> {
        let server_data = self.server_data.lock().unwrap();
        let workspace_files = server_data.workspace_files.borrow();
        let temp_files = server_data.temp_files.borrow();

        let file_path = params.text_document_position.text_document.uri.to_file_path().unwrap();

        let file: &dyn File = if let Some(workspace_file) = workspace_files.get(&file_path) {
            workspace_file
        } else if let Some(temp_file) = temp_files.get(&file_path) {
            temp_file
        } else {
            return None;
        };

        let content = file.content().borrow();
        let tree = file.tree().borrow();
        let line_mapping = file.line_mapping().borrow();

        TreeParser::find_references(
            &params.text_document_position.text_document.uri,
            params.text_document_position.position,
            &tree,
            &content,
            &line_mapping,
        )
    }
}
