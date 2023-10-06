use super::*;

fn abstract_include_path(pack_path: &Path, absolute_path: &Path) -> core::result::Result<String, ()> {
    let mut pack_path_components = pack_path.components();
    let mut absolute_path_components = absolute_path.components();

    loop {
        match (pack_path_components.next(), absolute_path_components.next()) {
            (Some(x), Some(y)) if x == y => (),
            (Some(_), Some(component)) => {
                let mut resource = "/../".to_owned();
                for _ in pack_path_components {
                    resource += "../";
                }
                resource += component.as_os_str().to_str().unwrap();
                for component in absolute_path_components {
                    resource.push('/');
                    resource += component.as_os_str().to_str().unwrap();
                }
                break Ok(resource);
            }
            (Some(_), None) => break Err(()),
            (None, Some(component)) => {
                let mut resource = "/".to_owned();
                resource += component.as_os_str().to_str().unwrap();
                for component in absolute_path_components {
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
    workspace_files: &HashMap<PathBuf, WorkspaceFile>, workspace_file: &WorkspaceFile, before_path: &Path, after_path: &Path,
    changes: &mut std::collections::HashMap<Url, Vec<TextEdit>>,
) {
    match abstract_include_path(workspace_file.pack_path(), after_path) {
        Ok(include_path) => {
            workspace_file.included_files().borrow().iter().for_each(|parent_path| {
                if let Some(parent_file) = workspace_files.get(parent_path) {
                    let url = Url::from_file_path(parent_path).unwrap();
                    let change_list = parent_file
                        .including_files()
                        .borrow()
                        .iter()
                        .filter(|(_, _, _, prev_include_path)| *before_path == *prev_include_path)
                        .map(|(line, start, end, _)| TextEdit {
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
                        })
                        .collect();
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
        let workspace_files = server_data.workspace_files.borrow_mut();

        let mut changes = std::collections::HashMap::new();

        for renamed_file in &params.files {
            let before_path = Url::parse(&renamed_file.old_uri).unwrap().to_file_path().unwrap();
            let after_path = Url::parse(&renamed_file.new_uri).unwrap().to_file_path().unwrap();

            if before_path.is_file() {
                if let Some(workspace_file) = workspace_files.get(&before_path) {
                    rename_file(&workspace_files, workspace_file, &before_path, &after_path, &mut changes);
                }
            } else {
                workspace_files.iter().for_each(|(file_path, workspace_file)| {
                    file_path.strip_prefix(&before_path).map_or((), |stripped_path| {
                        let after_path = after_path.join(stripped_path);
                        rename_file(&workspace_files, workspace_file, file_path, &after_path, &mut changes);
                    });
                });
            }
        }

        Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        })
    }
}
