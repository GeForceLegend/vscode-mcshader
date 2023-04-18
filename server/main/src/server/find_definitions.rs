use super::*;

impl MinecraftLanguageServer {
    pub fn find_definitions(&self, params: GotoDeclarationParams) -> Option<Vec<Location>> {
        let server_data = self.server_data.lock().unwrap();
        let workspace_files = server_data.workspace_files.borrow();
        let temp_files = server_data.temp_files.borrow();

        let file_path = params.text_document_position_params.text_document.uri.to_file_path().unwrap();
        let position = params.text_document_position_params.position;

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

        TreeParser::find_definitions(
            &params.text_document_position_params.text_document.uri,
            &position,
            &tree,
            &content,
            &line_mapping,
        )
    }
}
