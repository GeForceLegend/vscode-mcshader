use super::*;

impl MinecraftLanguageServer {
    pub fn document_links(&self, url: Url) -> Option<(Vec<DocumentLink>, HashMap<Url, Vec<Diagnostic>>)> {
        let file_path = url.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let workspace_files = server_data.workspace_files.borrow();
        let temp_files = server_data.temp_files.borrow();
        let temp_lint = server_data.temp_lint.borrow();

        let mut diagnostics = HashMap::new();
        let including_files = if let Some(workspace_file) = workspace_files.get(&file_path) {
            let mut shader_files = HashMap::new();
            workspace_file.get_base_shaders(&workspace_files, &mut shader_files, &file_path, 0);
            for (shader_path, shader_file) in shader_files {
                self.lint_workspace_shader(&workspace_files, shader_file, shader_path, &mut diagnostics);
            }

            workspace_file.including_files().borrow()
        } else if let Some(temp_file) = temp_files.get(&file_path) {
            diagnostics = self.lint_temp_file(temp_file, &file_path, url, *temp_lint);
            temp_file.including_files().borrow()
        } else {
            return None;
        };
        let include_links = including_files
            .iter()
            .map(|(line, start, end, include_path)| {
                let url = Url::from_file_path(include_path).unwrap();
                DocumentLink {
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
                    tooltip: Some(include_path.to_str().unwrap().to_owned()),
                    target: Some(url),
                    data: None,
                }
            })
            .collect::<Vec<_>>();

        Some((include_links, diagnostics))
    }
}
