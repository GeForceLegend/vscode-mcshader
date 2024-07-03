use super::*;

impl MinecraftLanguageServer {
    pub fn open_file(&self, params: DidOpenTextDocumentParams) {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        if let Some((file_path, workspace_file)) = workspace_files.get_key_value(&file_path) {
            let content = params.text_document.text;
            *workspace_file.tree().borrow_mut() = parser.parse(&content, None).unwrap();
            *workspace_file.line_mapping().borrow_mut() = generate_line_mapping(&content);
            *workspace_file.content().borrow_mut() = content;

            // Clone the content so they can be used alone.
            let file_path = file_path.clone();
            let workspace_file = workspace_file.clone();
            let mut update_list = HashMap::new();

            WorkspaceFile::parse_content(
                &mut workspace_files,
                &mut temp_files,
                &mut parser,
                &mut update_list,
                &workspace_file,
                &file_path,
                1,
            );
        } else {
            let temp_file = TempFile::new(&mut parser, &file_path, params.text_document.text);
            temp_files.insert(file_path, temp_file);
        }
    }
}
