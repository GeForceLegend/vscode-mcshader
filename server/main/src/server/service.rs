use std::borrow::Borrow;
use std::path::{PathBuf, MAIN_SEPARATOR_STR};

use hashbrown::{HashMap, HashSet};
use logging::{info, warn};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{request::*, *};
use tree_sitter::Parser;
use url::Url;

use crate::constant::*;
use crate::diagnostics_parser::DiagnosticsCollection;
use crate::file::*;
use crate::tree_parser::TreeParser;

use super::MinecraftLanguageServer;

impl MinecraftLanguageServer {
    /*================================================ Tool functions for service ================================================*/

    fn scan_new_file(
        &self, parser: &mut Parser, shader_packs: &HashSet<PathBuf>, workspace_files: &mut HashMap<PathBuf, WorkspaceFile>,
        temp_files: &mut HashMap<PathBuf, TempFile>, file_path: &PathBuf,
    ) -> bool {
        for shader_pack in shader_packs {
            if file_path.starts_with(shader_pack) {
                let relative_path = file_path.strip_prefix(shader_pack).unwrap();
                if DEFAULT_SHADERS.contains(relative_path.to_str().unwrap()) {
                    WorkspaceFile::new_shader(workspace_files, temp_files, parser, &shader_pack, &file_path);
                    return true;
                } else if let Some(result) = relative_path.to_str().unwrap().split_once(MAIN_SEPARATOR_STR) {
                    if RE_DIMENSION_FOLDER.is_match(result.0) && DEFAULT_SHADERS.contains(result.1) {
                        WorkspaceFile::new_shader(workspace_files, temp_files, parser, &shader_pack, &file_path);
                        return true;
                    }
                }
                return false;
            }
        }
        false
    }

    fn find_shader_packs(&self, shader_packs: &mut Vec<PathBuf>, curr_path: &PathBuf) {
        for file in curr_path.read_dir().unwrap() {
            if let Ok(file) = file {
                let file_path = file.path();
                if file_path.is_dir() {
                    if file.file_name() == "shaders" {
                        info!("Find shader pack {}", file_path.to_str().unwrap());
                        shader_packs.push(file_path);
                    } else {
                        self.find_shader_packs(shader_packs, &file_path);
                    }
                }
            }
        }
    }

    fn scan_files_in_root(
        &self, parser: &mut Parser, shader_packs: &mut HashSet<PathBuf>, workspace_files: &mut HashMap<PathBuf, WorkspaceFile>,
        temp_files: &mut HashMap<PathBuf, TempFile>, root: PathBuf,
    ) {
        info!("Generating file framework on current root"; "root" => root.to_str().unwrap());

        let mut sub_shader_packs: Vec<PathBuf> = vec![];
        if root.file_name().unwrap() == "shaders" {
            sub_shader_packs.push(root);
        } else {
            self.find_shader_packs(&mut sub_shader_packs, &root);
        }

        for pack_path in &sub_shader_packs {
            for file in pack_path.read_dir().unwrap() {
                if let Ok(file) = file {
                    let file_path = file.path();
                    if file_path.is_file() {
                        if DEFAULT_SHADERS.contains(file_path.file_name().unwrap().to_str().unwrap()) {
                            WorkspaceFile::new_shader(workspace_files, temp_files, parser, pack_path, &file_path);
                        }
                    } else if RE_DIMENSION_FOLDER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
                        for dim_file in file_path.read_dir().expect("read dimension folder failed") {
                            if let Ok(dim_file) = dim_file {
                                let file_path = dim_file.path();
                                if file_path.is_file() && DEFAULT_SHADERS.contains(file_path.file_name().unwrap().to_str().unwrap()) {
                                    WorkspaceFile::new_shader(workspace_files, temp_files, parser, pack_path, &file_path);
                                }
                            }
                        }
                    }
                }
            }
        }

        shader_packs.extend(sub_shader_packs);
    }

    fn lint_shader(
        &self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, shader_file: &WorkspaceFile, file_path: &PathBuf,
    ) -> HashMap<Url, Vec<Diagnostic>> {
        let mut file_list: HashMap<String, Url> = HashMap::new();
        let mut shader_content = String::new();
        shader_file.merge_file(workspace_files, &mut file_list, &mut shader_content, file_path, &mut -1, 0);
        preprocess_shader(&mut shader_content, shader_file.pack_path().borrow());

        let validation_result = OPENGL_CONTEXT.validate_shader(*shader_file.file_type().borrow(), shader_content);

        match validation_result {
            Some(compile_log) => {
                info!(
                    "Compilation errors reported; shader file: {},\nerrors: \"\n{}\"",
                    file_path.to_str().unwrap(),
                    compile_log
                );
                DIAGNOSTICS_PARSER.parse_diagnostics(compile_log, file_list, file_path)
            }
            None => {
                info!("Compilation reported no errors"; "shader file" => file_path.to_str().unwrap());
                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                diagnostics.insert(Url::from_file_path(file_path).unwrap(), vec![]);
                for include_file in file_list {
                    diagnostics.insert(include_file.1, vec![]);
                }
                diagnostics
            }
        }
    }

    fn temp_lint(&self, temp_file: &TempFile, file_path: &PathBuf) -> HashMap<Url, Vec<Diagnostic>> {
        let mut file_list: HashMap<String, Url> = HashMap::new();

        if let Some(result) = temp_file.merge_self(file_path, &mut file_list) {
            let validation_result = OPENGL_CONTEXT.validate_shader(result.0, result.1);

            match validation_result {
                Some(compile_log) => {
                    info!(
                        "Compilation errors reported; shader file: {},\nerrors: \"\n{}\"",
                        file_path.to_str().unwrap(),
                        compile_log
                    );
                    DIAGNOSTICS_PARSER.parse_diagnostics(compile_log, file_list, file_path)
                }
                None => {
                    info!("Compilation reported no errors"; "shader file" => file_path.to_str().unwrap());
                    let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                    diagnostics.insert(Url::from_file_path(file_path).unwrap(), vec![]);
                    for include_file in file_list {
                        diagnostics.insert(include_file.1, vec![]);
                    }
                    diagnostics
                }
            }
        } else {
            HashMap::new()
        }
    }

    /*================================================ Main service functions ================================================*/

    pub fn initial_scan(&self, roots: HashSet<PathBuf>) {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut shader_packs = server_data.shader_packs.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        for root in roots {
            self.scan_files_in_root(&mut parser, &mut shader_packs, &mut workspace_files, &mut temp_files, root);
        }

        *server_data.extensions.borrow_mut() = BASIC_EXTENSIONS.clone();
    }

    pub fn open_file(&self, file_path: PathBuf) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let workspace_files = server_data.workspace_files.borrow();
        let mut temp_files = server_data.temp_files.borrow_mut();

        if let Some(workspace_file) = workspace_files.get(&file_path) {
            let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
            let mut shader_files = HashMap::new();
            workspace_file.get_base_shaders(&workspace_files, &mut shader_files, &file_path, 0);

            for (shader_path, shader_file) in shader_files {
                diagnostics.extend(self.lint_shader(&workspace_files, shader_file, &shader_path));
            }
            Some(diagnostics)
        } else {
            if let Some(temp_file) = TempFile::new(&mut parser, &file_path) {
                let diagnostics = self.temp_lint(&temp_file, &file_path);
                temp_files.insert(file_path, temp_file);
                Some(diagnostics)
            } else {
                None
            }
        }
    }

    pub fn change_file(&self, file_path: &PathBuf, changes: Vec<TextDocumentContentChangeEvent>) {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        if let Some(workspace_file) = workspace_files.get(file_path) {
            workspace_file.apply_edit(changes, &mut parser);
            // Clone the content so they can be used alone.
            let pack_path = workspace_file.pack_path().clone();
            let content = workspace_file.content().borrow().clone();
            let old_including_files = workspace_file.including_files().borrow().iter().map(|including_data| {
                including_data.3.clone()
            }).collect::<HashSet<_>>();

            WorkspaceFile::update_include(&mut workspace_files, &mut temp_files, &mut parser, old_including_files, &content, &pack_path, file_path, 0);
        } else if let Some(temp_file) = temp_files.get(file_path) {
            temp_file.apply_edit(changes, &mut parser);
            temp_file.parse_includes(file_path);
        }
    }

    pub fn save_file(&self, file_path: PathBuf) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        if let Some(workspace_file) = workspace_files.get(&file_path) {
            if !server_data.extensions.borrow().contains(file_path.extension().unwrap().to_str().unwrap()) {
                workspace_file.update_from_disc(&mut parser, &file_path);
                // Clone the content so they can be used alone.
                let pack_path = workspace_file.pack_path().clone();
                let content = workspace_file.content().borrow().clone();
                let old_including_files = workspace_file.including_files().borrow().iter().map(|including_data| {
                    including_data.3.clone()
                }).collect::<HashSet<_>>();

                // Get the new pointer of this file (it might changed if workspace file list get reallocated).
                // workspace_files will not get modded after this call so this should be safe
                let workspace_file = 
                    WorkspaceFile::update_include(&mut workspace_files, &mut temp_files, &mut parser, old_including_files, &content, &pack_path, &file_path, 0);

                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                let mut shader_files = HashMap::new();
                unsafe { workspace_file.as_ref().unwrap().get_base_shaders(&workspace_files, &mut shader_files, &file_path, 0) };

                for (shader_path, shader_file) in shader_files {
                    diagnostics.extend_diagnostics(self.lint_shader(&workspace_files, shader_file, &shader_path));
                }
                return Some(diagnostics);
            }
        } else if let Some(temp_file) = temp_files.get_mut(&file_path) {
            temp_file.update_from_disc(&mut parser, &file_path);
            temp_file.parse_includes(&file_path);
            return Some(self.temp_lint(temp_file, &file_path));
        }

        return None;
    }

    pub fn close_file(&self, file_url: &Url) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let file_path = file_url.to_file_path().unwrap();

        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        // Force closing may result in temp changes discarded, so the content should reset to the disc copy.
        if let Some(workspace_file) = workspace_files.get(&file_path) {
            workspace_file.update_from_disc(&mut parser, &file_path);
            // Clone the content so they can be used alone.
            let pack_path = workspace_file.pack_path().clone();
            let content = workspace_file.content().borrow().clone();
            let old_including_files = workspace_file.including_files().borrow().iter().map(|including_data| {
                including_data.3.clone()
            }).collect::<HashSet<_>>();

            // Get the new pointer of this file (it might changed if workspace file list get reallocated).
            // workspace_files will not get modded after this call so this should be safe
            let workspace_file = 
                WorkspaceFile::update_include(&mut workspace_files, &mut temp_files, &mut parser, old_including_files, &content, &pack_path, &file_path, 0);

            let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
            let mut shader_files = HashMap::new();
            unsafe { workspace_file.as_ref().unwrap().get_base_shaders(&workspace_files, &mut shader_files, &file_path, 0) };

            for (shader_path, shader_file) in shader_files {
                diagnostics.extend_diagnostics(self.lint_shader(&workspace_files, shader_file, &shader_path));
            }
            return Some(diagnostics);
        }

        match temp_files.remove(&file_path) {
            Some(_) => Some(HashMap::from([(file_url.clone(), vec![])])),
            None => None,
        }
    }

    pub fn document_links(&self, file_path: &PathBuf) -> Option<Vec<DocumentLink>> {
        let server_data = self.server_data.lock().unwrap();
        let workspace_files = server_data.workspace_files.borrow();
        let temp_files = server_data.temp_files.borrow();

        let mut include_links = vec![];
        let including_files;
        if let Some(workspace_file) = workspace_files.get(file_path) {
            including_files = workspace_file.including_files().borrow();
        } else if let Some(temp_file) = temp_files.get(file_path) {
            including_files = temp_file.including_files().borrow();
        } else {
            return None;
        }
        for (line, start, end, include_path) in including_files.iter() {
            let url = Url::from_file_path(include_path).unwrap();

            include_links.push(DocumentLink {
                range: Range::new(Position::new(*line as u32, *start as u32), Position::new(*line as u32, *end as u32)),
                tooltip: Some(url.path().to_owned()),
                target: Some(url),
                data: None,
            });
        }

        Some(include_links)
    }

    pub fn find_definitions(&self, params: GotoDeclarationParams) -> Result<Option<Vec<Location>>> {
        let server_data = self.server_data.lock().unwrap();
        let workspace_files = server_data.workspace_files.borrow();
        let temp_files = server_data.temp_files.borrow();

        let file_path = params.text_document_position_params.text_document.uri.to_file_path().unwrap();
        let position = params.text_document_position_params.position;

        let file: &dyn File;
        if let Some(workspace_file) = workspace_files.get(&file_path) {
            file = workspace_file;
        } else if let Some(temp_file) = temp_files.get(&file_path) {
            file = temp_file;
        } else {
            return Ok(None);
        }

        let content = file.content().borrow();
        let tree = file.tree().borrow();
        let line_mapping = file.line_mapping().borrow();

        TreeParser::find_definitions(
            &params.text_document_position_params.text_document.uri,
            &position,
            &tree,
            &content,
            &line_mapping,
        )
    }

    pub fn find_references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let server_data = self.server_data.lock().unwrap();
        let workspace_files = server_data.workspace_files.borrow();
        let temp_files = server_data.temp_files.borrow();

        let file_path = params.text_document_position.text_document.uri.to_file_path().unwrap();
        let position = params.text_document_position.position;

        let file: &dyn File;
        if let Some(workspace_file) = workspace_files.get(&file_path) {
            file = workspace_file;
        } else if let Some(temp_file) = temp_files.get(&file_path) {
            file = temp_file;
        } else {
            return Ok(None);
        }

        let content = file.content().borrow();
        let tree = file.tree().borrow();
        let line_mapping = file.line_mapping().borrow();

        TreeParser::find_references(
            &params.text_document_position.text_document.uri,
            &position,
            &tree,
            &content,
            &line_mapping,
        )
    }

    pub fn update_work_spaces(&self, events: WorkspaceFoldersChangeEvent) {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut shader_packs = server_data.shader_packs.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        for removed_workspace in events.removed {
            let removed_path = removed_workspace.uri.to_file_path().unwrap();
            workspace_files.retain(|file_path, _include| !file_path.starts_with(&removed_path));
        }

        for added_workspace in events.added {
            let added_path = added_workspace.uri.to_file_path().unwrap();
            self.scan_files_in_root(&mut parser, &mut shader_packs, &mut workspace_files, &mut temp_files, added_path);
        }
    }

    pub fn update_watched_files(&self, changes: Vec<FileEvent>) -> HashMap<Url, Vec<Diagnostic>> {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();
        let shader_packs = server_data.shader_packs.borrow();
        let extensions = server_data.extensions.borrow();

        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        let mut updated_shaders = HashSet::new();

        let mut change_list: HashSet<PathBuf> = HashSet::new();

        for change in changes {
            let file_path = change.uri.to_file_path().unwrap();
            if change.typ == FileChangeType::CHANGED {
                if let Some(workspace_file) = workspace_files.get(&file_path) {
                    workspace_file.update_from_disc(&mut parser, &file_path);
                    // Clone the content so they can be used alone.
                    let pack_path = workspace_file.pack_path().clone();
                    let content = workspace_file.content().borrow().clone();
                    let old_including_files = workspace_file.including_files().borrow().iter().map(|including_data| {
                        including_data.3.clone()
                    }).collect::<HashSet<_>>();

                    // Get the new pointer of this file (it might changed if workspace file list get reallocated).
                    // workspace_files will not get modded after this call so this should be safe
                    let workspace_file = 
                        WorkspaceFile::update_include(&mut workspace_files, &mut temp_files, &mut parser, old_including_files, &content, &pack_path, &file_path, 0);

                    unsafe { workspace_file.as_ref().unwrap().get_base_shader_pathes(&workspace_files, &mut updated_shaders, &file_path, 0) };
                }
            } else {
                // Insert them to a hashset and handle later
                // This will prevent from multiple handling
                // when a file is deleted and created at the same time (eg.switch git branch)
                change_list.insert(file_path);
            }
        }

        for file_path in change_list {
            // Files that created or refreshed though delete and create again will exist
            // Otherwise it is deleted
            if file_path.exists() {
                if let Some(workspace_file) = workspace_files.get(&file_path) {
                    workspace_file.update_from_disc(&mut parser, &file_path);
                    // Clone the content so they can be used alone.
                    let pack_path = workspace_file.pack_path().clone();
                    let content = workspace_file.content().borrow().clone();
                    let old_including_files = workspace_file.including_files().borrow().iter().map(|including_data| {
                        including_data.3.clone()
                    }).collect::<HashSet<_>>();

                    // Get the new pointer of this file (it might changed if workspace file list get reallocated).
                    // workspace_files will not get modded after this call so this should be safe
                    let workspace_file = 
                        WorkspaceFile::update_include(&mut workspace_files, &mut temp_files, &mut parser, old_including_files, &content, &pack_path, &file_path, 0);

                    unsafe { workspace_file.as_ref().unwrap().get_base_shader_pathes(&workspace_files, &mut updated_shaders, &file_path, 0) };
                }
                if self.scan_new_file(&mut parser, &shader_packs, &mut workspace_files, &mut temp_files, &file_path) {
                    updated_shaders.insert(file_path);
                }
            } else {
                // If a path is not watched through extension, it might be a folder
                let is_watched_file = match file_path.extension() {
                    Some(ext) => extensions.contains(ext.to_str().unwrap()),
                    None => false,
                };
                // Folder handling is much more expensive than file handling
                // Almost nobody will name a folder with watched extension, right?
                if is_watched_file {
                    diagnostics.insert(Url::from_file_path(&file_path).unwrap(), vec![]);

                    if let Some(workspace_file) = workspace_files.get(&file_path) {
                        workspace_file.get_base_shader_pathes(&workspace_files, &mut updated_shaders, &file_path, 0);
                        workspace_file.clear(&mut parser);
                    }
                    workspace_files.values().for_each(|workspace_file| {
                        workspace_file.included_files().borrow_mut().remove(&file_path);
                    });

                    updated_shaders.remove(&file_path);
                } else {
                    workspace_files.iter().filter(|workspace_file| {
                        workspace_file.0.starts_with(&file_path)
                    })
                    .for_each(|(file_path, workspace_file)| {
                        diagnostics.insert(Url::from_file_path(file_path).unwrap(), vec![]);
                        workspace_file.get_base_shader_pathes(&workspace_files, &mut updated_shaders, &file_path, 0);
                        workspace_file.clear(&mut parser);

                        workspace_files.values().for_each(|workspace_file| {
                            workspace_file.included_files().borrow_mut().remove(file_path);
                        });
                    });
                    // There might some include files inserted deleted shader into update list.
                    updated_shaders.retain(|shader_path| {
                        !shader_path.starts_with(&file_path)
                    });
                }
            }
        }
        for file_path in updated_shaders {
            match workspace_files.get(&file_path) {
                Some(shader_file) => diagnostics.extend_diagnostics(self.lint_shader(&workspace_files, shader_file, &file_path)),
                None => warn!("Missing shader: {}", file_path.to_str().unwrap()),
            }
        }

        diagnostics
    }
}
