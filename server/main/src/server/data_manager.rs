use std::{
    path::PathBuf,
    collections::{HashMap, HashSet, LinkedList}
};

use logging::warn;
use tower_lsp::lsp_types::{Diagnostic, WorkspaceFoldersChangeEvent, FileEvent, FileChangeType};
use url::Url;

use crate::{diagnostics_parser::DiagnosticsParser, opengl::OpenGlContext};
use crate::enhancer::FromUrl;

use super::server_data::ServerData;

pub trait DataManager {
    fn initial_scan(&self, roots: HashSet<PathBuf>);
    fn open_file(&self, file_path: &PathBuf, diagnostics_parser: &DiagnosticsParser) -> Option<HashMap<Url, Vec<Diagnostic>>>;
    fn save_file(&self, file_path: &PathBuf, diagnostics_parser: &DiagnosticsParser) -> Option<HashMap<Url, Vec<Diagnostic>>>;
    fn include_list(&self, file_path: &PathBuf) -> Option<LinkedList<(usize, usize, usize, PathBuf)>>;
    fn update_work_spaces(&self, events: WorkspaceFoldersChangeEvent);
    fn update_watched_files(&self, changes: Vec<FileEvent>, diagnostics_parser: &DiagnosticsParser) -> HashMap<Url, Vec<Diagnostic>>;
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

    fn open_file(&self, file_path: &PathBuf, diagnostics_parser: &DiagnosticsParser) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        if shader_files.contains_key(file_path) || include_files.contains_key(file_path) {
            let opengl_context = OpenGlContext::new();
            return Some(self.update_lint(&mut shader_files, &mut include_files, file_path, &opengl_context, diagnostics_parser));
        }
        return None;
    }

    fn save_file(&self, file_path: &PathBuf, diagnostics_parser: &DiagnosticsParser) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        if shader_files.contains_key(file_path) || include_files.contains_key(file_path) {
            let opengl_context = OpenGlContext::new();
            self.update_file(&mut shader_files, &mut include_files, &file_path);
            return Some(self.update_lint(&mut shader_files, &mut include_files, file_path, &opengl_context, diagnostics_parser));
        }

        return None;
    }

    fn include_list(&self, file_path: &PathBuf) -> Option<LinkedList<(usize, usize, usize, PathBuf)>> {
        let shader_files = self.shader_files().lock().unwrap();
        let include_files = self.include_files().lock().unwrap();

        if let Some(shader_file) = shader_files.get(file_path) {
            return Some(shader_file.including_files().clone());
        }
        else if let Some(include_file) = include_files.get(file_path) {
            return Some(include_file.including_files().clone());
        }
        else {
            return None;
        }
    }
    
    fn update_work_spaces(&self, events: WorkspaceFoldersChangeEvent) {
        let mut roots = self.roots().lock().unwrap();
        let mut shader_packs = self.shader_packs().lock().unwrap();
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        for removed_uri in events.removed {
            let removed_path = PathBuf::from_url(removed_uri.uri);
            roots.remove(&removed_path);
            for shader in shader_files.clone() {
                if shader.0.starts_with(&removed_path) {
                    self.remove_shader_file(&mut shader_files, &mut include_files, &shader.0);
                }
            }
        }
        for added_uri in events.added {
            let added_path = PathBuf::from_url(added_uri.uri);
            self.scan_files_in_root(&mut shader_packs, &mut shader_files, &mut include_files, &added_path);
            roots.insert(added_path);
        }
    }

    fn update_watched_files(&self, changes: Vec<FileEvent>, diagnostics_parser: &DiagnosticsParser) -> HashMap<Url, Vec<Diagnostic>> {
        let mut shader_packs = self.shader_packs().lock().unwrap();
        let mut shader_files = self.shader_files().lock().unwrap();
        let mut include_files = self.include_files().lock().unwrap();

        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        let opengl_context = OpenGlContext::new();

        for change in changes {
            let file_path = PathBuf::from_url(change.uri);
            match change.typ {
                FileChangeType::CREATED => {
                    self.scan_new_file(&mut shader_packs, &mut shader_files, &mut include_files, file_path.clone());
                    diagnostics.extend(self.update_lint(&mut shader_files, &mut include_files, &file_path, &opengl_context, diagnostics_parser));
                },
                FileChangeType::CHANGED => {
                    self.update_file(&mut shader_files, &mut include_files, &file_path);
                    diagnostics.extend(self.update_lint(&mut shader_files, &mut include_files, &file_path, &opengl_context, diagnostics_parser));
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

        diagnostics
    }
}
