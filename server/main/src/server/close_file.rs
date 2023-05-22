use super::*;

impl MinecraftLanguageServer {
    pub fn close_file(&self, file_url: Url) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let file_path = file_url.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        // Force closing may result in temp changes discarded, so the content should reset to the disc copy.
        if let Some(workspace_file) = workspace_files.get(&file_path) {
            workspace_file.update_from_disc(&mut parser, &file_path);
            // Clone the content so they can be used alone.
            let pack_path = workspace_file.pack_path().clone();
            let content = workspace_file.content().borrow().clone();
            let old_including_files = workspace_file.including_pathes();
            let parent_shaders = workspace_file.parent_shaders().borrow().clone();

            // Get the new pointer of this file (it might changed if workspace file list get reallocated).
            // workspace_files will not get modded after this call so this should be safe
            let workspace_file = WorkspaceFile::update_include(
                &mut workspace_files,
                &mut temp_files,
                &mut parser,
                old_including_files,
                &parent_shaders,
                &content,
                &pack_path,
                &file_path,
                0,
            );

            let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
            let shader_files = 
            unsafe {
                workspace_file
                    .as_ref()
                    .unwrap()
                    .parent_shaders()
                    .borrow()
            };

            shader_files
                .iter()
                .filter_map(|shader_path| workspace_files.get(shader_path).map(|shader_file| (shader_path, shader_file)))
                .for_each(|(shader_path, shader_file)| {
                    self.lint_workspace_shader(&workspace_files, shader_file, shader_path, &mut diagnostics);
                });

            // self.update_diagnostics(&workspace_files, &temp_files, &diagnostics);
            self.collect_memory(&mut workspace_files);
            return Some(diagnostics);
        }

        temp_files.remove(&file_path).map(|_| HashMap::from([(file_url, vec![])]))
    }
}
