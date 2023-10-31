use super::*;

impl MinecraftLanguageServer {
    pub fn close_file(&self, file_url: Url) -> Option<Diagnostics> {
        let file_path = file_url.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        // Force closing may result in temp changes discarded, so the content should reset to the disc copy.
        let diagnostics = if let Some((file_path, workspace_file)) = workspace_files.get_key_value(&file_path) {
            workspace_file.update_from_disc(&mut parser, file_path);
            // Clone the content so they can be used alone.
            let file_path = file_path.clone();
            let mut old_including_files = workspace_file.including_pathes();
            let parent_shaders = workspace_file.parent_shaders().borrow().clone();

            let workspace_file = workspace_file.clone();
            WorkspaceFile::update_include(
                &mut workspace_files,
                &mut temp_files,
                &mut parser,
                &workspace_file,
                &mut old_including_files,
                &parent_shaders,
                &file_path,
                1,
            );

            let shader_files = workspace_file.parent_shaders().borrow();

            let mut update_list = old_including_files;
            shader_files
                .iter()
                .for_each(|(shader_path, shader_file)| {
                    self.lint_workspace_shader(shader_file, shader_path, &mut update_list);
                });

            let diagnostics = self.collect_diagnostics(&update_list);
            Some(diagnostics)
        } else {
            temp_files.remove(&file_path).map(|_| HashMap::from([(file_url, vec![])]))
        };

        self.collect_memory(&mut workspace_files);
        diagnostics
    }
}
