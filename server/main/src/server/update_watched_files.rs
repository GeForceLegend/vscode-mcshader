use super::*;

impl MinecraftLanguageServer {
    pub fn update_watched_files(&self, changes: Vec<FileEvent>) -> Diagnostics {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();
        let shader_packs = server_data.shader_packs.borrow();
        let extensions = server_data.extensions.borrow();

        let mut updated_shaders = HashSet::new();
        let mut change_list = HashSet::new();
        let mut update_list = HashSet::new();

        for change in &changes {
            let file_path = change.uri.to_file_path().unwrap();
            if change.typ != FileChangeType::CHANGED {
                // Insert them to a hashset and handle later
                // This will prevent from multiple handling
                // when a file is deleted and created at the same time (eg.switch git branch)
                change_list.insert(file_path);
            } else if let Some(workspace_file) = workspace_files.get(&file_path) {
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
                    1,
                );
                let workspace_file = workspace_files.get(&file_path).unwrap();
                *workspace_file.including_files().borrow_mut() = new_including_files;

                update_list.extend(old_including_files);
                updated_shaders.extend(workspace_file.parent_shaders().borrow().iter().cloned());
            }
        }

        for file_path in change_list {
            // Files that created or refreshed though delete and create again will exist
            // Otherwise it is deleted
            if file_path.exists() {
                if let Some(workspace_file) = workspace_files.get(&file_path) {
                    workspace_file.update_from_disc(&mut parser, &file_path);
                    {
                        let mut file_type = workspace_file.file_type().borrow_mut();
                        if *file_type == gl::INVALID_ENUM {
                            *file_type = gl::NONE;
                        }
                    }
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
                    );
                    let workspace_file = workspace_files.get(&file_path).unwrap();
                    *workspace_file.including_files().borrow_mut() = new_including_files;

                    update_list.extend(old_including_files);
                    updated_shaders.extend(workspace_file.parent_shaders().borrow().iter().cloned());
                }
                if self.scan_new_file(&mut parser, &shader_packs, &mut workspace_files, &mut temp_files, &file_path) {
                    updated_shaders.insert(file_path);
                }
            } else {
                // If a path is not watched through extension, it might be a folder
                let is_watched_file = file_path
                    .extension()
                    .map_or(false, |ext| extensions.contains(ext.to_str().unwrap()));
                // Folder handling is much more expensive than file handling
                // Almost nobody will name a folder with watched extension, right?
                if is_watched_file {
                    if let Some(workspace_file) = workspace_files.get(&file_path) {
                        updated_shaders.extend(workspace_file.parent_shaders().borrow().iter().cloned());
                        workspace_file.clear(&mut parser);
                    }
                    workspace_files.values().for_each(|workspace_file| {
                        workspace_file.included_files().borrow_mut().remove(&file_path);
                        workspace_file.parent_shaders().borrow_mut().remove(&file_path);
                    });

                    updated_shaders.remove(&file_path);
                    update_list.insert(file_path);
                } else {
                    update_list.extend(
                        workspace_files
                            .iter()
                            .filter(|workspace_file| workspace_file.0.starts_with(&file_path))
                            .map(|(file_path, workspace_file)| {
                                updated_shaders.extend(workspace_file.parent_shaders().borrow().iter().cloned());
                                workspace_file.clear(&mut parser);
                                workspace_files.values().for_each(|workspace_file| {
                                    workspace_file.included_files().borrow_mut().remove(file_path);
                                    workspace_file.parent_shaders().borrow_mut().remove(file_path);
                                });
                                file_path.clone()
                            }),
                    );
                    // There might be some include files inserting deleted shader into update list before the shaders get deleted in later loop.
                    updated_shaders.retain(|shader_path| !shader_path.starts_with(&file_path));
                }
            }
        }
        for file_path in &updated_shaders {
            match workspace_files.get(file_path) {
                Some(shader_file) => self.lint_workspace_shader(&workspace_files, shader_file, file_path, &mut update_list),
                None => warn!("Missing shader: {}", file_path.to_str().unwrap()),
            }
        }
        let diagnostics = self.collect_diagnostics(&workspace_files, &update_list);

        self.collect_memory(&mut workspace_files);
        diagnostics
    }
}
