use std::{
    path::PathBuf,
    collections::{HashMap, HashSet}, sync::Mutex
};

use logging::warn;
use path_slash::PathBufExt;
use tower_lsp::lsp_types::*;
use url::Url;

use crate::constant::RE_MACRO_INCLUDE;
use crate::diagnostics_parser::DiagnosticsParser;
use crate::opengl::OpenGlContext;
use crate::file::TempFile;

use super::data::{ServerData, extend_diagnostics};

fn parse_includes(content: &String, pack_path: &PathBuf, file_path: &PathBuf) -> Vec<DocumentLink> {
    let mut include_links = Vec::new();

    content.lines()
        .enumerate()
        .for_each(|line| {
            if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                let cap = capture.get(1).unwrap();
                let path: String = cap.as_str().into();

                let start = cap.start();
                let end = cap.end();

                let include_path = match path.strip_prefix('/') {
                    Some(path) => pack_path.join(PathBuf::from_slash(path)),
                    None => file_path.parent().unwrap().join(PathBuf::from_slash(&path))
                };
                let url = Url::from_file_path(include_path).unwrap();

                include_links.push(DocumentLink {
                    range: Range::new(
                        Position::new(u32::try_from(line.0).unwrap(), u32::try_from(start).unwrap()),
                        Position::new(u32::try_from(line.0).unwrap(), u32::try_from(end).unwrap()),
                    ),
                    tooltip: Some(url.path().to_string()),
                    target: Some(url),
                    data: None,
                });
            }
        });
    include_links
}

impl ServerData {
    pub fn initial_scan(&self, roots: HashSet<PathBuf>) {
        let mut work_space_roots = self.roots().lock().unwrap();
        let mut shader_packs = self.shader_packs().lock().unwrap();
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        for root in &roots {
            self.scan_files_in_root(&mut shader_packs, &mut shader_files, &mut include_files, root);
        }

        *work_space_roots = roots;
    }

    pub fn open_file(&self, file_path: &PathBuf) {
        let shader_files = self.shader_files().lock().unwrap();
        let include_files = self.include_files().lock().unwrap();
        let mut temp_files = self.temp_files().lock().unwrap();

        if !shader_files.contains_key(file_path) && !include_files.contains_key(file_path) {
            if let Some(temp_file) = TempFile::new(file_path) {
                temp_files.insert(file_path.clone(), temp_file);
            }
        }
    }

    pub fn change_file(&self, file_path: &PathBuf, changes: Vec<TextDocumentContentChangeEvent>) {
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();
        let mut temp_files = self.temp_files().lock().unwrap();

        let content;
        if let Some(shader_file) = shader_files.get_mut(file_path) {
            content = shader_file.content_mut();
        }
        else if let Some(include_file) = include_files.get_mut(file_path) {
            content = include_file.content_mut();
        }
        else if let Some(temp_file) = temp_files.get_mut(file_path) {
            content = temp_file.content_mut();
        }
        else {
            return;
        }

        #[cfg(target_os = "windows")]
        const NEW_LINE_LENGTH: usize = 2;
        #[cfg(not(target_os = "windows"))]
        const NEW_LINE_LENGTH: usize = 1;

        let mut total_content: usize = 0;
        let mut line_location: Vec<usize> = Vec::new();
        content.lines()
            .for_each(|line| {
                line_location.push(total_content.clone());
                total_content += line.len() + NEW_LINE_LENGTH;
            });

        changes.iter()
            .for_each(|change| {
                let start = line_location.get(change.range.unwrap().start.line as usize).unwrap() + change.range.unwrap().start.character as usize;
                let end = start + change.range_length.unwrap() as usize;
                content.replace_range(start..end, &change.text);
            });
    }

    pub fn save_file(&self, file_path: &PathBuf, extensions: &Mutex<HashSet<String>>,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext
    ) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();
        let mut temp_files = self.temp_files().lock().unwrap();
        let extensions = extensions.lock().unwrap();

        // Leave the files with watched extension to get linted by did_change_watched_files event
        // If this file does not exist in file system, return None to enable temp lint.
        if extensions.contains(file_path.extension().unwrap().to_str().unwrap()) && (include_files.contains_key(file_path) || shader_files.contains_key(file_path)) {
            return Some(HashMap::new());
        }
        else if let Some(mut include_file) = include_files.remove(file_path) {
            include_file.update_include(&mut include_files);
            include_files.insert(file_path.clone(), include_file);
            return Some(self.update_lint(&mut shader_files, &mut include_files, file_path, opengl_context, diagnostics_parser));
        }
        else if let Some(temp_file) = temp_files.get_mut(file_path) {
            temp_file.update_self();
            return Some(self.temp_lint(&temp_file, opengl_context, diagnostics_parser));
        }

        return None;
    }

    pub fn close_file(&self, file_path: &PathBuf) {
        self.temp_files().lock().unwrap().remove(file_path);
    }

    pub fn document_links(&self, file_path: &PathBuf,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext
    ) -> Option<(Vec<DocumentLink>, HashMap<Url, Vec<Diagnostic>>)> {
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();
        let temp_files = self.temp_files().lock().unwrap();

        let mut diagnostics = self.update_lint(&mut shader_files, &mut include_files, file_path, opengl_context, diagnostics_parser);

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
        else if let Some(temp_file) = temp_files.get(file_path) {
            content = temp_file.content();
            pack_path = temp_file.pack_path();
            extend_diagnostics(&mut diagnostics, self.temp_lint(&temp_file, opengl_context, diagnostics_parser));
        }
        else {
            return None;
        }
        let include_links = parse_includes(content, pack_path, file_path);

        Some((include_links, diagnostics))
    }

    pub fn update_work_spaces(&self, events: WorkspaceFoldersChangeEvent) {
        let mut roots = self.roots().lock().unwrap();
        let mut shader_packs = self.shader_packs().lock().unwrap();
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        events.removed.iter()
            .for_each(|removed_file|{
                let removed_path = removed_file.uri.to_file_path().unwrap();
                roots.remove(&removed_path);
                for shader in shader_files.clone() {
                    if shader.0.starts_with(&removed_path) {
                        self.remove_shader_file(&mut shader_files, &mut include_files, &shader.0);
                    }
                }
            });

        events.added.iter()
            .for_each(|added_file| {
                let added_path = added_file.uri.to_file_path().unwrap();
                self.scan_files_in_root(&mut shader_packs, &mut shader_files, &mut include_files, &added_path);
                roots.insert(added_path);
            });
    }

    pub fn update_watched_files(&self, changes: Vec<FileEvent>,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext
    ) -> HashMap<Url, Vec<Diagnostic>> {
        let mut shader_packs = self.shader_packs().lock().unwrap();
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        let mut updated_shaders: HashSet<PathBuf> = HashSet::new();

        changes.iter()
            .for_each(|change| {
                let file_path = change.uri.to_file_path().unwrap();
                match change.typ {
                    FileChangeType::CREATED => {
                        if self.scan_new_file(&mut shader_packs, &mut shader_files, &mut include_files, file_path.clone()) {
                            updated_shaders.insert(file_path);
                        }
                    },
                    FileChangeType::CHANGED => {
                        if let Some(mut include_file) = include_files.remove(&file_path) {
                            include_file.update_include(&mut include_files);
                            updated_shaders.extend(include_file.included_shaders().clone());
                            include_files.insert(file_path.clone(), include_file);
                        }
                        if let Some(shader_file) = shader_files.get_mut(&file_path) {
                            updated_shaders.insert(file_path);
                            shader_file.read_file(&mut include_files);
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
            });

        for file_path in updated_shaders {
            extend_diagnostics(&mut diagnostics, self.lint_shader(&mut shader_files, &mut include_files, &file_path, opengl_context, diagnostics_parser));
        }

        diagnostics
    }
}
