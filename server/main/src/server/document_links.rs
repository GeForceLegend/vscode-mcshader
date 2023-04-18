use super::*;

impl MinecraftLanguageServer {
    pub fn document_links(&self, file_path: &PathBuf) -> Option<Vec<DocumentLink>> {
        let server_data = self.server_data.lock().unwrap();
        let workspace_files = server_data.workspace_files.borrow();
        let temp_files = server_data.temp_files.borrow();

        let including_files = if let Some(workspace_file) = workspace_files.get(file_path) {
            workspace_file.including_files().borrow()
        } else if let Some(temp_file) = temp_files.get(file_path) {
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
                    tooltip: Some(url.path().to_owned()),
                    target: Some(url),
                    data: None,
                }
            })
            .collect::<Vec<_>>();

        Some(include_links)
    }
}
