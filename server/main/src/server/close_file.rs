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

            // Get the new pointer of this file (it might changed if workspace file list get reallocated).
            // workspace_files will not get modded after this call so this should be safe
            let workspace_file = WorkspaceFile::update_include(
                &mut workspace_files,
                &mut temp_files,
                &mut parser,
                old_including_files,
                &content,
                &pack_path,
                &file_path,
                0,
            );

            let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
            let mut shader_files = HashMap::new();
            unsafe {
                workspace_file
                    .as_ref()
                    .unwrap()
                    .get_base_shaders(&workspace_files, &mut shader_files, &file_path, 0)
            };

            for (shader_path, shader_file) in shader_files {
                self.lint_workspace_shader(&workspace_files, shader_file, shader_path, &mut diagnostics);
            }

            // self.update_diagnostics(&workspace_files, &temp_files, &diagnostics);
            self.collect_memory(&mut workspace_files);
            return Some(diagnostics);
        }

        temp_files.remove(&file_path).map(|_| HashMap::from([(file_url, vec![])]))
    }
}
