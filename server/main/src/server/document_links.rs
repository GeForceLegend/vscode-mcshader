use super::*;

impl MinecraftLanguageServer {
    pub fn document_links(&self, url: Url) -> Option<(Vec<DocumentLink>, Diagnostics)> {
        let file_path = url.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let workspace_files = server_data.workspace_files.borrow();
        let temp_files = server_data.temp_files.borrow();
        let temp_lint = server_data.temp_lint.borrow();

        let (include_links, diagnostics) = if let Some(workspace_file) = workspace_files.get(&file_path) {
            let shader_files = workspace_file.parent_shaders().borrow();
            let mut update_list = HashMap::new();
            shader_files.iter().for_each(|(shader_path, shader_file)| {
                self.lint_workspace_shader(shader_file, shader_path, &mut update_list);
            });

            (workspace_file.include_links(), self.collect_diagnostics(&update_list))
        } else {
            let temp_file = temp_files.get(&file_path)?;
            (
                temp_file.include_links(),
                self.lint_temp_file(temp_file, &file_path, url, *temp_lint),
            )
        };

        Some((include_links, diagnostics))
    }
}
