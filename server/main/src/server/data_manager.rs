use std::{
    path::PathBuf,
    collections::{HashMap, HashSet}
};

use logging::warn;
use tower_lsp::lsp_types::*;
use url::Url;

use crate::diagnostics_parser::DiagnosticsParser;
use crate::opengl::OpenGlContext;
use crate::shader_file::parse_includes;

use super::server_data::{ServerData, extend_diagnostics};

pub trait DataManager {
    fn initial_scan(&self, roots: HashSet<PathBuf>);

    fn open_file(&self, file_path: &PathBuf,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext
    ) -> Option<HashMap<Url, Vec<Diagnostic>>>;

    fn change_file(&self, file_path: &PathBuf, changes: Vec<TextDocumentContentChangeEvent>);

    fn save_file(&self, file_path: &PathBuf, extensions: &HashSet<String>,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext)
         -> Option<HashMap<Url, Vec<Diagnostic>>>;

    fn include_links(&self, file_path: &PathBuf) -> Option<Vec<DocumentLink>>;

    fn update_work_spaces(&self, events: WorkspaceFoldersChangeEvent);

    fn update_watched_files(&self, changes: Vec<FileEvent>,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext
    ) -> HashMap<Url, Vec<Diagnostic>>;
}

impl DataManager for ServerData {
    fn initial_scan(&self, roots: HashSet<PathBuf>) {
        let mut work_space_roots = self.roots().lock().unwrap();
        let mut shader_packs = self.shader_packs().lock().unwrap();
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        for root in &roots {
            self.scan_files_in_root(&mut shader_packs, &mut shader_files, &mut include_files, root);
        }

        *work_space_roots = roots;
    }

    fn open_file(&self, file_path: &PathBuf,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext
    ) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        if shader_files.contains_key(file_path) || include_files.contains_key(file_path) {
            return Some(self.update_lint(&mut shader_files, &mut include_files, file_path, opengl_context, diagnostics_parser));
        }
        return None;
    }

    fn change_file(&self, file_path: &PathBuf, changes: Vec<TextDocumentContentChangeEvent>) {
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        let content;
        if let Some(shader_file) = shader_files.get_mut(file_path) {
            content = shader_file.content_mut();
        }
        else if let Some(include_file) = include_files.get_mut(file_path) {
            content = include_file.content_mut();
        }
        else {
            return;
        }

        let new_line_length: usize;
        if content.contains("\r\n") {
            new_line_length = 2;
        }
        else {
            new_line_length = 1;
        }

        let mut total_content: usize = 0;
        let mut line_location: Vec<usize> = Vec::new();
        content.lines()
            .for_each(|line| {
                line_location.push(total_content.clone());
                total_content += line.len() + new_line_length;
            });

        for change in changes {
            let start = line_location.get(change.range.unwrap().start.line as usize).unwrap() + change.range.unwrap().start.character as usize;
            let end = start + change.range_length.unwrap() as usize;
            content.replace_range(start..end, &change.text);
        }
    }

    fn save_file(&self, file_path: &PathBuf, extensions: &HashSet<String>,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext
    ) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        // Leave the files with watched extension to get linted by did_change_watched_files event
        // If this file does not exist in file system, return None to enable temp lint.
        if extensions.contains(file_path.extension().unwrap().to_str().unwrap()) && (include_files.contains_key(file_path) || shader_files.contains_key(file_path)) {
            return Some(HashMap::new());
        }
        else if include_files.contains_key(file_path) {
            self.update_file(&mut shader_files, &mut include_files, file_path);
            return Some(self.update_lint(&mut shader_files, &mut include_files, file_path, opengl_context, diagnostics_parser));
        }

        return None;
    }

    fn include_links(&self, file_path: &PathBuf) -> Option<Vec<DocumentLink>> {
        let shader_files = self.shader_files().lock().unwrap();
        let include_files = self.include_files().lock().unwrap();

        let content;
        let pack_path;
        if let Some(shader_file) = shader_files.get(file_path) {
            content = shader_file.content();
            pack_path = shader_file.pack_path();
        }
        else if let Some(include_file) = include_files.get(file_path) {
            content = include_file.content();
            pack_path = include_file.pack_path();
        }
        else {
            return None;
        }

        Some(parse_includes(content, pack_path, file_path))
    }

    fn update_work_spaces(&self, events: WorkspaceFoldersChangeEvent) {
        let mut roots = self.roots().lock().unwrap();
        let mut shader_packs = self.shader_packs().lock().unwrap();
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        for removed_uri in events.removed {
            let removed_path = removed_uri.uri.to_file_path().unwrap();
            roots.remove(&removed_path);
            for shader in shader_files.clone() {
                if shader.0.starts_with(&removed_path) {
                    self.remove_shader_file(&mut shader_files, &mut include_files, &shader.0);
                }
            }
        }
        for added_uri in events.added {
            let added_path = added_uri.uri.to_file_path().unwrap();
            self.scan_files_in_root(&mut shader_packs, &mut shader_files, &mut include_files, &added_path);
            roots.insert(added_path);
        }
    }

    fn update_watched_files(&self, changes: Vec<FileEvent>,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext
    ) -> HashMap<Url, Vec<Diagnostic>> {
        let mut shader_packs = self.shader_packs().lock().unwrap();
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        let mut updated_shaders: HashSet<PathBuf> = HashSet::new();

        for change in changes {
            let file_path = change.uri.to_file_path().unwrap();
            match change.typ {
                FileChangeType::CREATED => {
                    self.scan_new_file(&mut shader_packs, &mut shader_files, &mut include_files, file_path.clone());
                    if shader_files.contains_key(&file_path) {
                        updated_shaders.insert(file_path);
                    }
                },
                FileChangeType::CHANGED => {
                    self.update_file(&mut shader_files, &mut include_files, &file_path);
                    match include_files.get(&file_path) {
                        Some(include_file) => {
                            updated_shaders.extend(include_file.included_shaders().clone());
                        },
                        None => {
                            if shader_files.contains_key(&file_path) {
                                updated_shaders.insert(file_path);
                            }
                        }
                    }
                },
                FileChangeType::DELETED => {
                    diagnostics.insert(Url::from_file_path(&file_path).unwrap(), Vec::new());
                    if shader_files.contains_key(&file_path) {
                        self.remove_shader_file(&mut shader_files, &mut include_files, &file_path);
                    }
                },
                _ => warn!("Invalid change type")
            }
        }

        for file_path in updated_shaders {
            extend_diagnostics(&mut diagnostics, self.lint_shader(&mut shader_files, &mut include_files, &file_path, opengl_context, diagnostics_parser));
        }

        diagnostics
    }
}
