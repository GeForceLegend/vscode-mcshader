use super::*;

impl MinecraftLanguageServer {
    pub fn save_file(&self, url: Url) -> Option<Diagnostics> {
        let file_path = url.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();
        let temp_lint = server_data.temp_lint.borrow();
        let extensions = server_data.extensions.borrow();

        let diagnostics = if let Some((file_path, workspace_file)) = workspace_files.get_key_value(&file_path) {
            // If this file is ended with watched extension, it should get updated through update_watched_files
            if file_path.extension().map_or(true, |ext| extensions.contains(ext.to_str().unwrap())) {
                return None;
            }
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
            shader_files.iter().for_each(|(shader_path, shader_file)| {
                self.lint_workspace_shader(shader_file, shader_path, &mut update_list);
            });
            self.collect_diagnostics(&update_list)
        } else {
            let temp_file = temp_files.get(&file_path)?;
            temp_file.update_from_disc(&mut parser, &file_path);
            temp_file.parse_includes(&file_path);
            self.lint_temp_file(temp_file, &file_path, url, *temp_lint)
        };

        self.collect_memory(&mut workspace_files);
        Some(diagnostics)
    }
}
