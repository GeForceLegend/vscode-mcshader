use super::*;

impl MinecraftLanguageServer {
    pub fn close_file(&self, file_url: Url) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let file_path = file_url.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        // Force closing may result in temp changes discarded, so the content should reset to the disc copy.
        let diagnostics = if let Some(workspace_file) = workspace_files.get(&file_path) {
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
            drop(shader_files);

            let diagnostics = self.collect_diagnostics(&workspace_files, &update_list);
            self.collect_memory(&mut workspace_files);
            Some(diagnostics)
        } else {
            temp_files.remove(&file_path).map(|_| HashMap::from([(file_url, vec![])]))
        };

        diagnostics
    }
}
