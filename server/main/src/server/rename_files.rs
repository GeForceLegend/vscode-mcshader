use super::*;

fn abstract_include_path(pack_path: &Path, absolute_path: &Path) -> core::result::Result<String, ()> {
    let mut pack_path_components = pack_path.components();
    let mut absolute_path_components = absolute_path.components();

    loop {
        match (pack_path_components.next(), absolute_path_components.next()) {
            (Some(x), Some(y)) if x == y => (),
            (Some(_), Some(component)) => {
                let mut resource = "/../".to_owned();
                while let Some(_) = pack_path_components.next() {
                    resource += "../";
                }
                resource += component.as_os_str().to_str().unwrap();
                while let Some(component) = absolute_path_components.next() {
                    resource.push('/');
                    resource += component.as_os_str().to_str().unwrap();
                }
                break Ok(resource);
            }
            (Some(_), None) => break Err(()),
            (None, Some(component)) => {
                let mut resource = "/".to_owned();
                resource += component.as_os_str().to_str().unwrap();
                while let Some(component) = absolute_path_components.next() {
                    resource.push('/');
                    resource += component.as_os_str().to_str().unwrap();
                }
                break Ok(resource);
            }
            (None, None) => break Err(()),
        }
    }
}

fn rename_file(
    workspace_files: &HashMap<PathBuf, WorkspaceFile>, workspace_file: &WorkspaceFile, before_path: &PathBuf, after_path: &Path,
    changes: &mut HashMap<Url, Vec<TextEdit>>,
) {
    match abstract_include_path(workspace_file.pack_path(), after_path) {
        Ok(include_path) => {
            workspace_file.included_files().borrow().iter().for_each(|parent_path| {
                if let Some(parent_file) = workspace_files.get(parent_path) {
                    let url = Url::from_file_path(parent_path).unwrap();
                    let mut change_list = vec![];
                    parent_file
                        .including_files()
                        .borrow_mut()
                        .iter_mut()
                        .filter(|(_, _, _, prev_include_path)| *before_path == *prev_include_path)
                        .for_each(|(line, start, end, prev_include_path)| {
                            let edit: TextEdit = TextEdit {
                                range: Range {
                                    start: Position {
                                        line: *line as u32,
                                        character: *start as u32,
                                    },
                                    end: Position {
                                        line: *line as u32,
                                        character: *end as u32,
                                    },
                                },
                                new_text: include_path.clone(),
                            };
                            change_list.push(edit);
                            *end = *start + include_path.len();
                            *prev_include_path = after_path.to_path_buf();
                        });
                    if let Some(change) = changes.get_mut(&url) {
                        change.extend(change_list);
                    } else {
                        changes.insert(url, change_list);
                    }
                }
            });
        }
        Err(_) => error!("Cannot generate include path from new path"),
    };
}

impl MinecraftLanguageServer {
    pub fn rename_files(&self, params: RenameFilesParams) -> Option<WorkspaceEdit> {
        let server_data = self.server_data.lock().unwrap();
        let mut workspace_files = server_data.workspace_files.borrow_mut();

        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        let mut rename_list: HashMap<PathBuf, PathBuf> = HashMap::new();

        for renamed_file in &params.files {
            let before_path = Url::parse(&renamed_file.old_uri).unwrap().to_file_path().unwrap();
            let after_path = Url::parse(&renamed_file.new_uri).unwrap().to_file_path().unwrap();

            if before_path.is_file() {
                if let Some(workspace_file) = workspace_files.get(&before_path) {
                    rename_file(&workspace_files, workspace_file, &before_path, &after_path, &mut changes);
                    rename_list.insert(after_path, before_path);
                }
            } else {
                rename_list.extend(workspace_files.iter().filter_map(|(file_path, workspace_file)| {
                    match file_path.strip_prefix(&before_path) {
                        Ok(stripped_path) => {
                            let after_path = after_path.join(stripped_path);
                            rename_file(&workspace_files, workspace_file, file_path, &after_path, &mut changes);
                            Some((after_path, file_path.clone()))
                        }
                        Err(_) => None,
                    }
                }));
            }
        }
        // Only modify parents when all changes are pushed to list
        // Or modification may targets to modified url when an renamed file includes another renamed one
        for (after_path, before_path) in &rename_list {
            let workspace_file = workspace_files.get(before_path).unwrap();
            workspace_file
                .including_files()
                .borrow()
                .iter()
                .for_each(|(_, _, _, include_path)| {
                    if let Some(include_file) = workspace_files.get(include_path) {
                        let mut parent_list = include_file.included_files().borrow_mut();
                        parent_list.remove(before_path);
                        parent_list.insert(after_path.clone());
                    } else if let Some(path) = rename_list.get(include_path) {
                        // Since generating change list modifies including list, this may targets to a path after rename
                        let mut parent_list = workspace_files.get(path).unwrap().included_files().borrow_mut();
                        parent_list.remove(before_path);
                        parent_list.insert(after_path.clone());
                    }
                });
        }
        // Move them after all modifications on including and included lists are applied
        // This will avoid targeting modified path, causing applying edits or updating parents failed
        for (after_path, before_path) in rename_list {
            let workspace_file = workspace_files.remove(&before_path).unwrap();
            workspace_files.insert(after_path, workspace_file);
        }

        Some(WorkspaceEdit {
            changes: Some(std::collections::HashMap::from_iter(changes)),
            document_changes: None,
            change_annotations: None,
        })
    }
}
