use std::{
    collections::{HashMap, HashSet},
    path::{PathBuf, MAIN_SEPARATOR_STR},
};

use logging::{info, warn};
use path_slash::PathBufExt;
use tower_lsp::lsp_types::{*, request::*};
use tree_sitter::{Point, Parser, InputEdit};
use url::Url;

use crate::{constant::*, tree_parser::TreeParser};
use crate::diagnostics_parser::DiagnosticsParser;
use crate::opengl::OpenGlContext;
use crate::file::{ShaderFile, IncludeFile, TempFile};

use super::MinecraftLanguageServer;

fn generate_line_mapping(content: &String) -> Vec<usize> {
    let mut line_mapping: Vec<usize> = vec![0];
    for (i, char) in content.char_indices() {
        if char == '\n' {
            line_mapping.push(i + 1);
        }
    }
    line_mapping
}

fn parse_includes(content: &String, pack_path: &PathBuf, file_path: &PathBuf) -> Vec<DocumentLink> {
    let mut include_links = Vec::new();

    content.lines()
        .enumerate()
        .for_each(|line| {
            if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                let cap = capture.get(1).unwrap();
                let path = cap.as_str();

                let start = cap.start();
                let end = cap.end();

                let include_path = match path.strip_prefix('/') {
                    Some(path) => pack_path.join(PathBuf::from_slash(path)),
                    None => file_path.parent().unwrap().join(PathBuf::from_slash(path))
                };
                let url = Url::from_file_path(include_path).unwrap();

                include_links.push(DocumentLink {
                    range: Range::new(
                        Position::new(u32::try_from(line.0).unwrap(), u32::try_from(start).unwrap()),
                        Position::new(u32::try_from(line.0).unwrap(), u32::try_from(end).unwrap()),
                    ),
                    tooltip: Some(String::from(url.path())),
                    target: Some(url),
                    data: None,
                });
            }
        });
    include_links
}

pub fn extend_diagnostics(target: &mut HashMap<Url, Vec<Diagnostic>>, source: HashMap<Url, Vec<Diagnostic>>) {
    for file in source {
        if let Some(diagnostics) = target.get_mut(&file.0) {
            diagnostics.extend(file.1);
        }
        else {
            target.insert(file.0, file.1);
        }
    }
}

impl MinecraftLanguageServer {

    /*================================================ Tool functions for service ================================================*/

    fn add_shader_file(&self, shader_files: &mut HashMap<PathBuf,ShaderFile>,
        include_files: &mut HashMap<PathBuf,IncludeFile>, parser: &mut Parser, pack_path: &PathBuf, file_path: PathBuf
    ) {
        let shader_file = ShaderFile::new(include_files, parser, pack_path, &file_path);
        shader_files.insert(file_path, shader_file);
    }

    pub fn scan_new_file(&self, shader_files: &mut HashMap<PathBuf,ShaderFile>,
        include_files: &mut HashMap<PathBuf,IncludeFile>, parser: &mut Parser, shader_packs: &HashSet<PathBuf>, file_path: PathBuf
    ) -> bool {
        for shader_pack in shader_packs.iter() {
            if file_path.starts_with(&shader_pack) {
                let relative_path = file_path.strip_prefix(&shader_pack).unwrap();
                if DEFAULT_SHADERS.contains(relative_path.to_str().unwrap()) {
                    self.add_shader_file(shader_files, include_files, parser, &shader_pack, file_path);
                    return true;
                }
                else if let Some(result) = relative_path.to_str().unwrap().split_once(MAIN_SEPARATOR_STR) {
                    if RE_DIMENSION_FOLDER.is_match(result.0) && DEFAULT_SHADERS.contains(result.1) {
                        self.add_shader_file(shader_files, include_files, parser, &shader_pack, file_path);
                        return true;
                    }
                }
                return false;
            }
        }
        false
    }

    fn find_shader_packs(&self, curr_path: &PathBuf) -> Vec<PathBuf> {
        let mut shader_packs: Vec<PathBuf> = Vec::new();
        for file in curr_path.read_dir().expect("read directory failed") {
            if let Ok(file) = file {
                let file_path = file.path();
                if file_path.is_dir() {
                    if file_path.file_name().unwrap() == "shaders" {
                        info!("Find shader pack {}", file_path.display());
                        shader_packs.push(file_path);
                    }
                    else {
                        shader_packs.extend(self.find_shader_packs(&file_path));
                    }
                }
            }
        }
        shader_packs
    }

    pub fn scan_files_in_root(&self, shader_files: &mut HashMap<PathBuf,ShaderFile>,
        include_files: &mut HashMap<PathBuf,IncludeFile>, parser: &mut Parser, shader_packs: &mut HashSet<PathBuf>, root: &PathBuf
    ) {
        info!("Generating file framework on current root"; "root" => root.display());

        let sub_shader_packs: Vec<PathBuf>;
        if root.file_name().unwrap() == "shaders" {
            sub_shader_packs = Vec::from([root.clone()]);
        }
        else {
            sub_shader_packs = self.find_shader_packs(root);
        }

        for shader_pack in &sub_shader_packs {
            for file in shader_pack.read_dir().expect("read work space failed") {
                if let Ok(file) = file {
                    let file_path = file.path();
                    if file_path.is_file() && DEFAULT_SHADERS.contains(file_path.file_name().unwrap().to_str().unwrap()){
                        self.add_shader_file(shader_files, include_files, parser, shader_pack, file_path);
                    }
                    else if RE_DIMENSION_FOLDER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
                        for dim_file in file_path.read_dir().expect("read dimension folder failed") {
                            if let Ok(dim_file) = dim_file {
                                let file_path = dim_file.path();
                                if file_path.is_file() && DEFAULT_SHADERS.contains(file_path.file_name().unwrap().to_str().unwrap()){
                                    self.add_shader_file(shader_files, include_files, parser, shader_pack, file_path);
                                }
                            }
                        }
                    }
                }
            }
        }

        shader_packs.extend(sub_shader_packs);
    }

    pub fn lint_shader(&self, include_files: &HashMap<PathBuf, IncludeFile>, shader_file: &ShaderFile,
        file_path: &PathBuf, opengl_context: &OpenGlContext, diagnostics_parser: &DiagnosticsParser
    ) -> HashMap<Url, Vec<Diagnostic>> {
        let mut file_list: HashMap<String, PathBuf> = HashMap::new();
        let shader_content = shader_file.merge_shader_file(include_files, file_path, &mut file_list);

        let validation_result = opengl_context.validate_shader(shader_file.file_type(), &shader_content);

        match validation_result {
            Some(compile_log) => {
                info!("Compilation errors reported"; "errors" => format!("`{}`", compile_log.replace('\n', "\\n")), "shader file" => file_path.display());
                diagnostics_parser.parse_diagnostics(compile_log, file_list)
            },
            None => {
                info!("Compilation reported no errors"; "shader file" => file_path.display());
                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                diagnostics.entry(Url::from_file_path(file_path).unwrap()).or_default();
                for include_file in file_list {
                    diagnostics.entry(Url::from_file_path(&include_file.1).unwrap()).or_default();
                }
                diagnostics
            }
        }
    }

    pub fn temp_lint(&self, temp_file: &TempFile, file_path: &PathBuf,
        opengl_context: &OpenGlContext, diagnostics_parser: &DiagnosticsParser
    ) -> HashMap<Url, Vec<Diagnostic>> {
        let mut file_list: HashMap<String, PathBuf> = HashMap::new();

        if let Some(result) = temp_file.merge_self(file_path, &mut file_list) {
            let validation_result = opengl_context.validate_shader(result.0, &result.1);

            match validation_result {
                Some(compile_log) => {
                    info!("Compilation errors reported"; "errors" => format!("`{}`", compile_log.replace('\n', "\\n")), "shader file" => file_path.display());
                    diagnostics_parser.parse_diagnostics(compile_log, file_list)
                },
                None => {
                    info!("Compilation reported no errors"; "shader file" => file_path.display());
                    let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                    diagnostics.entry(Url::from_file_path(file_path).unwrap()).or_default();
                    for include_file in file_list {
                        diagnostics.entry(Url::from_file_path(&include_file.1).unwrap()).or_default();
                    }
                    diagnostics
                }
            }
        }
        else {
            HashMap::new()
        }
    }


    /*================================================ Main service functions ================================================*/

    pub fn initial_scan(&self, roots: HashSet<PathBuf>, extensions: HashSet<String>) {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut shader_packs = server_data.shader_packs.borrow_mut();
        let mut shader_files = server_data.shader_files.borrow_mut();
        let mut include_files = server_data.include_files.borrow_mut();

        for root in &roots {
            self.scan_files_in_root(&mut shader_files, &mut include_files, &mut parser, &mut shader_packs, root);
        }

        *server_data.roots.borrow_mut() = roots;
        *server_data.extensions.borrow_mut() = extensions;
    }

    pub fn open_file(&self, file_path: PathBuf) {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();

        if !server_data.shader_files.borrow().contains_key(&file_path) && !server_data.include_files.borrow().contains_key(&file_path) {
            if let Some(temp_file) = TempFile::new(&mut parser, &file_path) {
                server_data.temp_files.borrow_mut().insert(file_path, temp_file);
            }
        }
    }

    pub fn change_file(&self, file_path: &PathBuf, changes: Vec<TextDocumentContentChangeEvent>) {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut shader_files = server_data.shader_files.borrow_mut();
        let mut include_files = server_data.include_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        let mut content;
        let mut tree;
        if let Some(shader_file) = shader_files.get_mut(file_path) {
            content = shader_file.content().borrow_mut();
            tree = shader_file.tree().borrow_mut();
        }
        else if let Some(include_file) = include_files.get_mut(file_path) {
            content = include_file.content().borrow_mut();
            tree = include_file.tree().borrow_mut();
        }
        else if let Some(temp_file) = temp_files.get_mut(file_path) {
            content = temp_file.content().borrow_mut();
            tree = temp_file.tree().borrow_mut();
        }
        else {
            return;
        }

        let line_mapping = generate_line_mapping(&content);

        changes.iter()
            .for_each(|change| {
                let range = change.range.unwrap();
                let start = line_mapping.get(range.start.line as usize).unwrap() + range.start.character as usize;
                let end = start + change.range_length.unwrap() as usize;

                let original_content = content.get(start .. end).unwrap().to_owned();
                content.replace_range(start..end, &change.text);

                let new_end_position = match change.text.matches("\n").count() {
                    0 => Point {
                        row: range.start.line as usize,
                        column: range.start.character as usize + change.text.len(),
                    },
                    lines => Point {
                        row: range.start.line as usize + lines - original_content.matches("\n").count(),
                        column: change.text.split("\n").collect::<Vec<_>>().last().unwrap().len(),
                    },
                };
                tree.edit(&InputEdit{
                    start_byte: start,
                    old_end_byte: end,
                    new_end_byte: start + change.text.len(),
                    start_position: Point { row: range.start.line as usize, column: range.start.character as usize },
                    old_end_position: Point { row: range.end.line as usize, column: range.end.character as usize },
                    new_end_position,
                })
            });
        *tree = parser.parse(content.as_bytes(), Some(&tree)).unwrap();
    }

    pub fn save_file(&self, file_path: PathBuf,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext
    ) -> Option<HashMap<Url, Vec<Diagnostic>>> {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let shader_files = server_data.shader_files.borrow();
        let mut include_files = server_data.include_files.borrow_mut();
        let mut temp_files = server_data.temp_files.borrow_mut();

        // Leave the files with watched extension to get linted by did_change_watched_files event
        // If this file does not exist in file system, enable temp lint.
        if server_data.extensions.borrow().contains(file_path.extension().unwrap().to_str().unwrap()) &&
            (include_files.contains_key(&file_path) || shader_files.contains_key(&file_path)
        ) {
            return Some(HashMap::new());
        }
        else if let Some(mut include_file) = include_files.remove(&file_path) {
            include_file.update_include(&mut include_files, &mut parser, &file_path);
            let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
            for shader_path in include_file.included_shaders().borrow().iter() {
                let shader_file = shader_files.get(shader_path).unwrap();
                extend_diagnostics(&mut diagnostics, self.lint_shader(&include_files, shader_file, shader_path, opengl_context, diagnostics_parser));
            }
            include_files.insert(file_path, include_file);
            return Some(diagnostics);
        }
        else if let Some(temp_file) = temp_files.get_mut(&file_path) {
            temp_file.update_self(&file_path);
            return Some(self.temp_lint(&temp_file, &file_path, opengl_context, diagnostics_parser));
        }

        return None;
    }

    pub fn close_file(&self, file_path: &PathBuf) {
        self.server_data.lock().unwrap().temp_files.borrow_mut().remove(file_path);
    }

    pub fn document_links(&self, file_path: &PathBuf,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext
    ) -> Option<(Vec<DocumentLink>, HashMap<Url, Vec<Diagnostic>>)> {
        let server_data = self.server_data.lock().unwrap();
        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        let shader_files = server_data.shader_files.borrow();
        let include_files = server_data.include_files.borrow();
        let temp_files = server_data.temp_files.borrow();

        let content;
        let pack_path;
        if let Some(shader_file) = shader_files.get(file_path) {
            content = shader_file.content().borrow();
            pack_path = shader_file.pack_path();
            extend_diagnostics(&mut diagnostics, self.lint_shader(&include_files, shader_file, file_path, opengl_context, diagnostics_parser));
        }
        else if let Some(include_file) = include_files.get(file_path) {
            content = include_file.content().borrow();
            pack_path = include_file.pack_path();
            let include_shader_list = include_file.included_shaders().borrow();
            for shader_path in include_shader_list.iter() {
                let shader_file = shader_files.get(shader_path).unwrap();
                extend_diagnostics(&mut diagnostics, self.lint_shader(&include_files, shader_file, shader_path, opengl_context, diagnostics_parser));
            }
        }
        else if let Some(temp_file) = temp_files.get(file_path) {
            content = temp_file.content().borrow();
            pack_path = temp_file.pack_path();
            extend_diagnostics(&mut diagnostics, self.temp_lint(&temp_file, file_path, opengl_context, diagnostics_parser));
        }
        else {
            return None;
        }
        let include_links = parse_includes(&content, pack_path, file_path);

        Some((include_links, diagnostics))
    }

    pub fn find_definitions(&self, params: GotoDeclarationParams) -> Result<Option<Vec<Location>>, String> {
        let server_data = self.server_data.lock().unwrap();
        let shader_files = server_data.shader_files.borrow();
        let include_files = server_data.include_files.borrow();
        let temp_files = server_data.temp_files.borrow();

        let file_path = params.text_document_position_params.text_document.uri.to_file_path().unwrap();
        let position = params.text_document_position_params.position;

        let tree;
        let content;
        if let Some(include_file) = include_files.get(&file_path) {
            tree = include_file.tree().borrow();
            content = include_file.content().borrow();
        }
        else if let Some(shader_file) = shader_files.get(&file_path) {
            tree = shader_file.tree().borrow();
            content = shader_file.content().borrow();
        }
        else if let Some(temp_file) = temp_files.get(&file_path) {
            tree = temp_file.tree().borrow();
            content = temp_file.content().borrow();
        }
        else {
            return Err(String::from("Unable to load file content"));
        }

        TreeParser::find_definitions(&file_path, &position, &tree, &content)
    }

    pub fn find_references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>, String> {
        let server_data = self.server_data.lock().unwrap();
        let shader_files = server_data.shader_files.borrow();
        let include_files = server_data.include_files.borrow();
        let temp_files = server_data.temp_files.borrow();

        let file_path = params.text_document_position.text_document.uri.to_file_path().unwrap();
        let position = params.text_document_position.position;

        let tree;
        let content;
        if let Some(include_file) = include_files.get(&file_path) {
            tree = include_file.tree().borrow();
            content = include_file.content().borrow();
        }
        else if let Some(shader_file) = shader_files.get(&file_path) {
            tree = shader_file.tree().borrow();
            content = shader_file.content().borrow();
        }
        else if let Some(temp_file) = temp_files.get(&file_path) {
            tree = temp_file.tree().borrow();
            content = temp_file.content().borrow();
        }
        else {
            return Err(String::from("Unable to load file content"));
        }

        TreeParser::find_references(&file_path, &position, &tree, &content)
    }

    pub fn update_work_spaces(&self, events: WorkspaceFoldersChangeEvent) {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut roots = server_data.roots.borrow_mut();
        let mut shader_packs = server_data.shader_packs.borrow_mut();
        let mut shader_files = server_data.shader_files.borrow_mut();
        let mut include_files = server_data.include_files.borrow_mut();

        events.removed.iter()
            .for_each(|removed_file|{
                let removed_path = removed_file.uri.to_file_path().unwrap();
                roots.remove(&removed_path);
                shader_files.retain(|file_path, _shader| {
                    !file_path.starts_with(&removed_path)
                });
                include_files.retain(|file_path, _include| {
                    !file_path.starts_with(&removed_path)
                });
            });

        events.added.iter()
            .for_each(|added_file| {
                let added_path = added_file.uri.to_file_path().unwrap();
                self.scan_files_in_root(&mut shader_files, &mut include_files, &mut parser, &mut shader_packs, &added_path);
                roots.insert(added_path);
            });
    }

    pub fn update_watched_files(&self, changes: Vec<FileEvent>,
        diagnostics_parser: &DiagnosticsParser, opengl_context: &OpenGlContext
    ) -> HashMap<Url, Vec<Diagnostic>> {
        let server_data = self.server_data.lock().unwrap();
        let mut parser = server_data.tree_sitter_parser.borrow_mut();
        let mut shader_packs = server_data.shader_packs.borrow();
        let mut shader_files = server_data.shader_files.borrow_mut();
        let mut include_files = server_data.include_files.borrow_mut();

        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        let mut updated_shaders: HashSet<PathBuf> = HashSet::new();

        changes.iter()
            .for_each(|change| {
                let file_path = change.uri.to_file_path().unwrap();
                match change.typ {
                    FileChangeType::CREATED => {
                        if self.scan_new_file(&mut shader_files, &mut include_files, &mut parser, &mut shader_packs, file_path.clone()) {
                            updated_shaders.insert(file_path);
                        }
                    },
                    FileChangeType::CHANGED => {
                        if let Some(mut include_file) = include_files.remove(&file_path) {
                            include_file.update_include(&mut include_files, &mut parser, &file_path);
                            updated_shaders.extend(include_file.included_shaders().borrow().clone());
                            include_files.insert(file_path.clone(), include_file);
                        }
                        if let Some(shader_file) = shader_files.get_mut(&file_path) {
                            shader_file.update_shader(&mut include_files, &mut parser, &file_path);
                            updated_shaders.insert(file_path);
                        }
                    },
                    FileChangeType::DELETED => {
                        diagnostics.insert(Url::from_file_path(&file_path).unwrap(), Vec::new());
                        shader_files.remove(&file_path);
                        include_files.remove(&file_path);

                        include_files.values_mut()
                            .for_each(|include_file|{
                                include_file.included_shaders().borrow_mut().remove(&file_path);
                                include_file.including_files().borrow_mut().remove(&file_path);
                            });
                    },
                    _ => warn!("Invalid change type")
                }
            });

        for file_path in updated_shaders {
            let shader_file = shader_files.get(&file_path).unwrap();
            extend_diagnostics(&mut diagnostics, self.lint_shader(&include_files, shader_file, &file_path, opengl_context, diagnostics_parser));
        }

        diagnostics
    }
}
