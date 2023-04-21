use super::*;

fn abstract_include_path(pack_path: &PathBuf, absolute_path: &PathBuf) -> core::result::Result<String, ()> {
    let mut pack_path_components = pack_path.components();
    let mut absolute_path_components = absolute_path.components();

    let mut resource;
    loop {
        match (pack_path_components.next(), absolute_path_components.next()) {
            (Some(ref x), Some(ref y)) if x == y => (),
            (Some(_), Some(component)) => {
                resource = "/../".to_owned();
                while let Some(_) = pack_path_components.next() {
                    resource += "../";
                }
                resource += component.as_os_str().to_str().unwrap();
                while let Some(component) = absolute_path_components.next() {
                    resource.push('/');
                    resource += component.as_os_str().to_str().unwrap();
                }
                break;
            }
            (Some(_), None) => return Err(()),
            (None, Some(component)) => {
                resource = "/".to_owned();
                resource += component.as_os_str().to_str().unwrap();
                while let Some(component) = absolute_path_components.next() {
                    resource.push('/');
                    resource += component.as_os_str().to_str().unwrap();
                }
                break;
            }
            (None, None) => return Err(()),
        }
    }
    Ok(resource)
}

fn rename_file(
    workspace_files: &mut HashMap<PathBuf, WorkspaceFile>, before_path: &PathBuf, after_path: &PathBuf,
    changes: &mut HashMap<Url, Vec<TextEdit>>,
) -> bool {
    if let Some(workspace_file) = workspace_files.get(before_path) {
        match abstract_include_path(workspace_file.pack_path(), &after_path) {
            Ok(include_path) => {
                workspace_file.included_files().borrow().iter().for_each(|parent_path| {
                    if let Some(parent_file) = workspace_files.get(parent_path) {
                        let url = Url::from_file_path(parent_path).unwrap();
                        parent_file
                            .including_files()
                            .borrow_mut()
                            .iter_mut()
                            .for_each(|(line, start, end, prev_include_path)| {
                                if *before_path == *prev_include_path {
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
                                    if let Some(change) = changes.get_mut(&url) {
                                        change.push(edit)
                                    } else {
                                        changes.insert(url.clone(), vec![edit; 1]);
                                    }
                                    *end = *start + include_path.len();
                                    *prev_include_path = after_path.clone();
                                }
                            });
                    }
                });
            }
            Err(_) => error!("Cannot generate include path from new path"),
        };
        true
    } else {
        false
    }
}

impl MinecraftLanguageServer {
    pub fn rename_files(&self, params: RenameFilesParams) -> Option<WorkspaceEdit> {
        let server_data = self.server_data.lock().unwrap();
        let mut workspace_files = server_data.workspace_files.borrow_mut();

        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        let mut rename_list: HashMap<PathBuf, PathBuf> = HashMap::new();

        for renamed_file in params.files {
            let before_path = Url::parse(&renamed_file.old_uri).unwrap().to_file_path().unwrap();
            let after_path = Url::parse(&renamed_file.new_uri).unwrap().to_file_path().unwrap();

            if before_path.is_file() {
                if rename_file(&mut workspace_files, &before_path, &after_path, &mut changes) {
                    rename_list.insert(after_path, before_path);
                }
            } else {
                let update_list = workspace_files
                    .iter()
                    .filter_map(|(file_path, _)| match file_path.strip_prefix(&before_path) {
                        Ok(stripped_path) => Some((after_path.join(stripped_path), file_path.clone())),
                        Err(_) => None,
                    })
                    .collect::<HashMap<_, _>>();

                for (after_path, before_path) in &update_list {
                    rename_file(&mut workspace_files, before_path, after_path, &mut changes);
                }
                rename_list.extend(update_list);
            }
        }
        // Only modify parents when all changes are pushed to list
        // Or modification may targets to modified url
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
                        let mut parent_list = workspace_files.get(path).unwrap().included_files().borrow_mut();
                        parent_list.remove(before_path);
                        parent_list.insert(after_path.clone());
                    }
                });
        }
        // Move them after all modifications on including and included lists are applied
        // This will avoid targeting modified url, causing applying edits or updating parents failed
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
