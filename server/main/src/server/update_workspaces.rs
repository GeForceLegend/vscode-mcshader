use super::*;

impl MinecraftLanguageServer {
    pub fn update_workspaces(&self, events: WorkspaceFoldersChangeEvent) -> Diagnostics {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut shader_packs = server_data.shader_packs.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        let mut diagnostics: Diagnostics = HashMap::new();
        for removed_workspace in &events.removed {
            let removed_path = removed_workspace.uri.to_file_path().unwrap();
            workspace_files.retain(|file_path, workspace_file| {
                if workspace_file.pack_path().starts_with(&removed_path) {
                    diagnostics.insert(Url::from_file_path(file_path).unwrap(), vec![]);
                    false
                } else {
                    true
                }
            });
        }

        for added_workspace in events.added {
            let added_path = added_workspace.uri.to_file_path().unwrap();
            self.scan_files_in_root(&mut parser, &mut shader_packs, &mut workspace_files, &mut temp_files, added_path);
        }
        diagnostics
    }
}
