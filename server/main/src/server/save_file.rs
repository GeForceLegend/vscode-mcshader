use super::*;

impl MinecraftLanguageServer {
    pub fn save_file(&self, url: Url) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let file_path = url.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();
        let temp_lint = server_data.temp_lint.borrow();

        let diagnostics = if let Some(workspace_file) = workspace_files.get(&file_path) {
            // If this file is ended with watched extension, it should get updated through update_watched_files
            if !server_data
                .extensions
                .borrow()
                .contains(file_path.extension().unwrap().to_str().unwrap())
            {
                workspace_file.update_from_disc(&mut parser, &file_path);
                // Clone the content so they can be used alone.
                let pack_path = workspace_file.pack_path().clone();
                let content = workspace_file.content().borrow().clone();
                let mut old_including_files = workspace_file.including_pathes();
                let parent_shaders = workspace_file.parent_shaders().borrow().clone();

                let new_including_files = WorkspaceFile::update_include(
                    &mut workspace_files,
                    &mut temp_files,
                    &mut parser,
                    &mut old_including_files,
                    &parent_shaders,
                    &content,
                    &pack_path,
                    &file_path,
                    0,
                )
                .unwrap();
                let workspace_file = workspace_files.get(&file_path).unwrap();
                *workspace_file.including_files().borrow_mut() = new_including_files;

                let shader_files = workspace_file.parent_shaders().borrow();

                let mut update_list = old_including_files;
                shader_files
                    .iter()
                    .filter_map(|shader_path| workspace_files.get(shader_path).map(|shader_file| (shader_path, shader_file)))
                    .for_each(|(shader_path, shader_file)| {
                        self.lint_workspace_shader(&workspace_files, shader_file, shader_path, &mut update_list);
                    });
                self.collect_diagnostics(&workspace_files, &update_list)
            } else {
                return None;
            }
        } else if let Some(temp_file) = temp_files.get_mut(&file_path) {
            temp_file.update_from_disc(&mut parser, &file_path);
            temp_file.parse_includes(&file_path);
            self.lint_temp_file(temp_file, &file_path, url, *temp_lint)
        } else {
            return None;
        };

        self.collect_memory(&mut workspace_files);
        Some(diagnostics)
    }
}
