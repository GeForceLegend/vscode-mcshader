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
        &self, shader_files: &mut HashMap<PathBuf, ShaderFile>, include_files: &mut HashMap<PathBuf, IncludeFile>, parser: &mut Parser,
        shader_packs: &HashSet<PathBuf>, file_path: PathBuf,
    ) -> bool {
        // for shader_pack in shader_packs {
        //     if file_path.starts_with(shader_pack) {
        //         let relative_path = file_path.strip_prefix(shader_pack).unwrap();
        //         if DEFAULT_SHADERS.contains(relative_path.to_str().unwrap()) {
        //             self.add_shader_file(shader_files, include_files, parser, shader_pack, file_path);
        //             return true;
        //         } else if let Some(result) = relative_path.to_str().unwrap().split_once(MAIN_SEPARATOR_STR) {
        //             if RE_DIMENSION_FOLDER.is_match(result.0) && DEFAULT_SHADERS.contains(result.1) {
        //                 self.add_shader_file(shader_files, include_files, parser, shader_pack, file_path);
        //                 return true;
        //             }
        //         }
        //         return false;
        //     }
        // }
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
        &self, parser: &mut Parser, shader_packs: &mut HashSet<PathBuf>, shader_pathes: &mut HashSet<PathBuf>,
        workspace_files: &mut HashMap<PathBuf, WorkspaceFile>, root: PathBuf,
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
                            WorkspaceFile::new_shader(workspace_files, parser, pack_path, &file_path);
                        }
                    } else if RE_DIMENSION_FOLDER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
                        for dim_file in file_path.read_dir().expect("read dimension folder failed") {
                            if let Ok(dim_file) = dim_file {
                                let file_path = dim_file.path();
                                if file_path.is_file() && DEFAULT_SHADERS.contains(file_path.file_name().unwrap().to_str().unwrap()) {
                                    WorkspaceFile::new_shader(workspace_files, parser, pack_path, &file_path);
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
        &self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, parser: &mut Parser, shader_file: &WorkspaceFile, file_path: &PathBuf,
    ) -> HashMap<Url, Vec<Diagnostic>> {
        let mut file_list: HashMap<String, Url> = HashMap::new();
        let mut shader_content = String::new();
        let mut file_id = -1;
        shader_file.merge_file(workspace_files, parser, &mut file_list, &mut shader_content, file_path, &mut file_id, 0);
        preprocess_shader(&mut shader_content, shader_file.pack_path().borrow());

        let validation_result = OPENGL_CONTEXT.validate_shader(shader_file.file_type(), shader_content);

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
        let mut shader_pathes = server_data.shader_pathes.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();

        for root in roots {
            self.scan_files_in_root(&mut parser, &mut shader_packs, &mut shader_pathes, &mut workspace_files, root);
        }

        *server_data.extensions.borrow_mut() = BASIC_EXTENSIONS.clone();
    }

    pub fn open_file(&self, file_path: PathBuf) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let workspace_files = server_data.workspace_files.borrow();

        if let Some(workspace_file) = workspace_files.get(&file_path) {
            let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
            let mut shader_files = HashMap::new();
            workspace_file.get_base_shaders(&workspace_files, &mut shader_files, &file_path, 0);

            // get_base_shaders will not delete or add any workspace file, so these shoule be valid raw pointers.
            unsafe {
                for (shader_path, shader_file) in shader_files {
                    let shader_file = shader_file.as_ref().unwrap();
                    diagnostics.extend(self.lint_shader(&workspace_files, &mut parser, shader_file, &shader_path));
                }
            }
            Some(diagnostics)
        } else {
            // if let Some(temp_file) = TempFile::new(&mut parser, &file_path) {
            //     server_data.temp_files.borrow_mut().insert(file_path, temp_file);
            // }
            None
        }
    }

    pub fn change_file(&self, file_path: &PathBuf, changes: Vec<TextDocumentContentChangeEvent>) {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        // let shader_files = server_data.shader_files.borrow();
        // let include_files = server_data.include_files.borrow();
        // let temp_files = server_data.temp_files.borrow();

        if let Some(workspace_file) = workspace_files.get(file_path) {
            workspace_file.apply_edit(changes, &mut parser);
            // Clone the content so they can be used alone.
            let pack_path = workspace_file.pack_path().clone();
            let content = workspace_file.content().borrow().clone();
            let old_including_files = workspace_file.including_files().borrow().iter().map(|including_data| {
                including_data.3.clone()
            }).collect::<HashSet<_>>();

            // Get the new pointer of this file (it might changed if workspace file list get ) and change its including_files.
            // Above call will only modify its included_files so there is no data race.
            let including_files = 
                WorkspaceFile::update_include(&mut workspace_files, &mut parser, old_including_files, &content, &pack_path, file_path, 0);
            let workspace_file = workspace_files.get(file_path).unwrap();
            *workspace_file.including_files().borrow_mut() = including_files;
        }

        // if let Some(shader_file) = shader_files.get(file_path) {
        //     shader_file.apply_edit(changes, &mut parser);
        // } else if let Some(include_file) = include_files.get(file_path) {
        //     include_file.apply_edit(changes, &mut parser);
        // } else if let Some(temp_file) = temp_files.get(file_path) {
        //     temp_file.apply_edit(changes, &mut parser);
        // }
    }

    pub fn save_file(&self, file_path: PathBuf) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        // let shader_files = server_data.shader_files.borrow();
        // let mut include_files = server_data.include_files.borrow_mut();
        // let mut temp_files = server_data.temp_files.borrow_mut();

        if let Some(workspace_file) = workspace_files.get(&file_path) {
            // if !server_data.extensions.borrow().contains(file_path.extension().unwrap().to_str().unwrap()) {
                workspace_file.update_from_disc(&mut parser, &file_path);
                // Clone the content so they can be used alone.
                let pack_path = workspace_file.pack_path().clone();
                let content = workspace_file.content().borrow().clone();
                let old_including_files = workspace_file.including_files().borrow().iter().map(|including_data| {
                    including_data.3.clone()
                }).collect::<HashSet<_>>();

                // Get the new pointer of this file (it might changed if workspace file list get ) and change its including_files.
                // Above call will only modify its included_files so there is no data race.
                let including_files = 
                    WorkspaceFile::update_include(&mut workspace_files, &mut parser, old_including_files, &content, &pack_path, &file_path, 0);
                let workspace_file = workspace_files.get(&file_path).unwrap();
                *workspace_file.including_files().borrow_mut() = including_files;

                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                let mut shader_files = HashMap::new();
                workspace_file.get_base_shaders(&workspace_files, &mut shader_files, &file_path, 0);

                // get_base_shaders will not delete or add any workspace file, so these shoule be valid raw pointers.
                unsafe {
                    for (shader_path, shader_file) in shader_files {
                        let shader_file = shader_file.as_ref().unwrap();
                        diagnostics.extend(self.lint_shader(&mut workspace_files, &mut parser, shader_file, &shader_path));
                    }
                }
                return Some(diagnostics);
            // }
        }

        // // Leave the files with watched extension getting linted by did_change_watched_files event
        // if server_data
        //     .extensions
        //     .borrow()
        //     .contains(file_path.extension().unwrap().to_str().unwrap())
        //     && (include_files.contains_key(&file_path) || shader_files.contains_key(&file_path))
        // {
        //     return None;
        // } else if let Some(include_file) = include_files.remove(&file_path) {
        //     include_file.update_include(&mut include_files, &mut parser, &file_path);
        //     let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        //     for shader_path in include_file.included_shaders().borrow().iter() {
        //         let shader_file = shader_files.get(shader_path).unwrap();
        //         diagnostics.extend_diagnostics(self.lint_shader(&include_files, shader_file, shader_path));
        //     }
        //     include_files.insert(file_path, include_file);
        //     return Some(diagnostics);
        // // If this file does not exist in file system, enable temp lint.
        // } else if let Some(temp_file) = temp_files.get_mut(&file_path) {
        //     temp_file.update_self(&file_path);
        //     return Some(self.temp_lint(&temp_file, &file_path));
        // }

        return None;
    }

    pub fn close_file(&self, file_url: &Url) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let file_path = file_url.to_file_path().unwrap();
        // match self.server_data.lock().unwrap().temp_files.borrow_mut().remove(&file_path) {
        //     Some(_) => Some(HashMap::from([(file_url.clone(), vec![])])),
        //     None => None,
        // }
        None
    }

    pub fn document_links(&self, file_path: &PathBuf) -> Option<Vec<DocumentLink>> {
        let server_data = self.server_data.lock().unwrap();
        // let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        let workspace_files = server_data.workspace_files.borrow();
        // let shader_files = server_data.shader_files.borrow();
        // let include_files = server_data.include_files.borrow();
        // let temp_files = server_data.temp_files.borrow();

        let mut include_links = vec![];
        // let file: &dyn File;
        if let Some(workspace_file) = workspace_files.get(file_path) {
            for (line, start, end, include_path) in workspace_file.including_files().borrow().iter() {
                let url = Url::from_file_path(include_path).unwrap();

                include_links.push(DocumentLink {
                    range: Range::new(Position::new(*line as u32, *start as u32), Position::new(*line as u32, *end as u32)),
                    tooltip: Some(url.path().to_owned()),
                    target: Some(url),
                    data: None,
                });
            }
        }
        // if let Some(shader_file) = shader_files.get(file_path) {
        //     file = shader_file;
        //     diagnostics.extend_diagnostics(self.lint_shader(&include_files, shader_file, file_path));
        // } else if let Some(include_file) = include_files.get(file_path) {
        //     file = include_file;
        //     let include_shader_list = include_file.included_shaders().borrow();
        //     for shader_path in include_shader_list.iter() {
        //         let shader_file = shader_files.get(shader_path).unwrap();
        //         diagnostics.extend_diagnostics(self.lint_shader(&include_files, shader_file, shader_path));
        //     }
        // } else if let Some(temp_file) = temp_files.get(file_path) {
        //     file = temp_file;
        //     diagnostics.extend_diagnostics(self.temp_lint(&temp_file, file_path));
        // } else {
        //     warn!("This file cannot found in server data! File path: {}", file_path.to_str().unwrap());
        //     return (None, diagnostics);
        // }
        // let include_links = file.parse_includes(file_path);

        // (Some(include_links), diagnostics)
        Some(include_links)
    }

    pub fn find_definitions(&self, params: GotoDeclarationParams) -> Result<Option<Vec<Location>>> {
        let server_data = self.server_data.lock().unwrap();
        let workspace_files = server_data.workspace_files.borrow();
        // let shader_files = server_data.shader_files.borrow();
        // let include_files = server_data.include_files.borrow();
        // let temp_files = server_data.temp_files.borrow();

        let file_path = params.text_document_position_params.text_document.uri.to_file_path().unwrap();
        let position = params.text_document_position_params.position;

        let file: &dyn File;
        if let Some(workspace_file) = workspace_files.get(&file_path) {
            file = workspace_file;
        } else {
            return Ok(None);
        }
        // if let Some(include_file) = include_files.get(&file_path) {
        //     file = include_file;
        // } else if let Some(shader_file) = shader_files.get(&file_path) {
        //     file = shader_file;
        // } else if let Some(temp_file) = temp_files.get(&file_path) {
        //     file = temp_file;
        // } else {
        //     return Ok(None);
        // }
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
        // let shader_files = server_data.shader_files.borrow();
        // let include_files = server_data.include_files.borrow();
        // let temp_files = server_data.temp_files.borrow();

        let file_path = params.text_document_position.text_document.uri.to_file_path().unwrap();
        let position = params.text_document_position.position;

        let file: &dyn File;
        if let Some(workspace_file) = workspace_files.get(&file_path) {
            file = workspace_file;
        } else {
            return Ok(None);
        }
        // if let Some(include_file) = include_files.get(&file_path) {
        //     file = include_file;
        // } else if let Some(shader_file) = shader_files.get(&file_path) {
        //     file = shader_file;
        // } else if let Some(temp_file) = temp_files.get(&file_path) {
        //     file = temp_file;
        // } else {
        //     return Ok(None);
        // }
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
        let mut shader_pathes = server_data.shader_pathes.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();

        for removed_workspace in events.removed {
            let removed_path = removed_workspace.uri.to_file_path().unwrap();
            shader_pathes.retain(|file_path| !file_path.starts_with(&removed_path));
            workspace_files.retain(|file_path, _include| !file_path.starts_with(&removed_path));
        }

        for added_workspace in events.added {
            let added_path = added_workspace.uri.to_file_path().unwrap();
            self.scan_files_in_root(&mut parser, &mut shader_packs, &mut shader_pathes, &mut workspace_files, added_path);
        }
    }

    pub fn update_watched_files(&self, changes: Vec<FileEvent>) -> HashMap<Url, Vec<Diagnostic>> {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut shader_pathes = server_data.shader_pathes.borrow_mut();
        let mut workspace_files = server_data.workspace_files.borrow_mut();
        // let mut shader_packs = server_data.shader_packs.borrow();
        // let mut shader_files = server_data.shader_files.borrow_mut();
        // let mut include_files = server_data.include_files.borrow_mut();
        let extensions = server_data.extensions.borrow();

        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        // let mut updated_shaders: HashSet<PathBuf> = HashSet::new();
        // let mut updated_includes: HashSet<PathBuf> = HashSet::new();
        // let mut updated_related_shaders: HashSet<PathBuf> = HashSet::new();

        let mut change_list: HashSet<PathBuf> = HashSet::new();

        for change in changes {
            let file_path = change.uri.to_file_path().unwrap();
            if change.typ == FileChangeType::CHANGED {
                if let Some(workspace_file) = workspace_files.get(&file_path) {
                    workspace_file.update_from_disc(&mut parser, &file_path);
                    let workspace_file_ptr = workspace_file as *const WorkspaceFile;
                    // workspace_file_ptr is existing in workspace_files, so recursion within this call will only modifies its included_files. This should be safe.
                    // unsafe { workspace_file_ptr.as_ref().unwrap().update_from_disc(&mut parser, &file_path) };
                    // include_file.update_include(&mut include_files, &mut parser, &file_path);
                    // updated_includes.insert(file_path.clone());
                    // include_files.insert(file_path.clone(), include_file);
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
                // let include_exists = match include_files.remove(&file_path) {
                //     Some(include_file) => {
                //         include_file.update_include(&mut include_files, &mut parser, &file_path);
                //         updated_includes.insert(file_path.clone());
                //         include_files.insert(file_path.clone(), include_file);
                //         true
                //     }
                //     None => false,
                // };
                // if let Some(shader_file) = shader_files.get(&file_path) {
                //     shader_file.update_shader(&mut include_files, &mut parser, &file_path);
                //     updated_shaders.insert(file_path);
                // } else if !include_exists {
                //     if self.scan_new_file(
                //         &mut shader_files,
                //         &mut include_files,
                //         &mut parser,
                //         &mut shader_packs,
                //         file_path.clone(),
                //     ) {
                //         updated_shaders.insert(file_path);
                //     }
                // }
            } else {
                // If a path is not watched through extension, it might be a folder
                let is_watched_file = match file_path.extension() {
                    Some(ext) => extensions.contains(ext.to_str().unwrap()),
                    None => false,
                };
                // Folder handling is much more expensive than file handling
                // Almost nobody will name a folder with watched extension, right?
                if is_watched_file {
                    // diagnostics.insert(Url::from_file_path(&file_path).unwrap(), vec![]);

                    // shader_files.remove(&file_path);
                    // if let Some(include_file) = include_files.remove(&file_path) {
                    //     updated_related_shaders.extend(include_file.included_shaders().borrow().clone());
                    // }
                    // updated_related_shaders.remove(&file_path);

                    // include_files.values().for_each(|include_file| {
                    //     include_file.included_shaders().borrow_mut().remove(&file_path);
                    //     include_file.including_files().borrow_mut().remove(&file_path);
                    // });
                } else {
                    // shader_files.retain(|shader_path, _shader_file| {
                    //     if shader_path.starts_with(&file_path) {
                    //         diagnostics.insert(Url::from_file_path(shader_path).unwrap(), vec![]);
                    //         false
                    //     } else {
                    //         true
                    //     }
                    // });
                    // include_files.retain(|include_path, include_file| {
                    //     if include_path.starts_with(&file_path) {
                    //         diagnostics.insert(Url::from_file_path(include_path).unwrap(), vec![]);
                    //         updated_related_shaders.extend(include_file.included_shaders().borrow().clone());
                    //         false
                    //     } else {
                    //         true
                    //     }
                    // });
                    // updated_related_shaders.retain(|shader_path| !shader_path.starts_with(&file_path));

                    // include_files.values().for_each(|include_file| {
                    //     include_file
                    //         .included_shaders()
                    //         .borrow_mut()
                    //         .retain(|shader_path| !shader_path.starts_with(&file_path));
                    //     include_file
                    //         .including_files()
                    //         .borrow_mut()
                    //         .retain(|include_path| !include_path.starts_with(&file_path));
                    // });
                }
            }
        }
        // updated_shaders.extend(updated_related_shaders);
        // for file_path in updated_includes {
        //     match include_files.get(&file_path) {
        //         Some(include_file) => updated_shaders.extend(include_file.included_shaders().borrow().clone()),
        //         None => warn!("Missing include: {}", file_path.to_str().unwrap()),
        //     }
        // }
        // for file_path in updated_shaders {
        //     match shader_files.get(&file_path) {
        //         Some(shader_file) => diagnostics.extend_diagnostics(self.lint_shader(&include_files, shader_file, &file_path)),
        //         None => warn!("Missing shader: {}", file_path.to_str().unwrap()),
        //     }
        // }

        diagnostics
    }
}
