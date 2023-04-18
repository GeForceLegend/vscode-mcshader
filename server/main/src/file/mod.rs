use std::{
    cell::RefCell,
    ffi::OsString,
    fs::read_to_string,
    path::{Component, PathBuf, MAIN_SEPARATOR_STR},
};

use hashbrown::{HashMap, HashSet};
use itoa::{Buffer, Integer};
use logging::error;
use tower_lsp::lsp_types::*;
use tree_sitter::{InputEdit, Parser, Point, Tree};

use crate::constant::*;

mod temp_file;
mod workspace_file;

fn include_path_join(root_path: &PathBuf, curr_path: &PathBuf, additional: &str) -> Result<PathBuf, String> {
    let mut buffer: Vec<Component>;
    let additional = match additional.strip_prefix("/") {
        Some(path) => {
            buffer = root_path.components().collect();
            PathBuf::from(path)
        }
        None => {
            buffer = curr_path.components().collect();
            buffer.pop();
            PathBuf::from(additional)
        }
    };

    for component in additional.components() {
        match component {
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = buffer.pop() {
                } else {
                    return Err("Unable to find parent while creating include path".to_owned());
                }
            }
            Component::Normal(_) => buffer.push(component),
            Component::CurDir => {}
            _ => return Err("Invalid component in include path".to_owned()),
        }
    }

    let mut resource = OsString::new();
    let last = buffer.pop().unwrap();
    for component in buffer {
        resource.push(component);
        match component {
            Component::Prefix(_) | Component::RootDir => {}
            _ => resource.push(MAIN_SEPARATOR_STR),
        }
    }
    resource.push(last);

    Ok(PathBuf::from(resource))
}

fn generate_line_macro<I: Integer>(content: &mut String, line: I, file_id: &str, file_name: &str) {
    content.push_str("#line ");
    content.push_str(Buffer::new().format(line));
    content.push(' ');
    content.push_str(file_id);
    content.push_str("\t//");
    content.push_str(file_name);
}

fn generate_line_mapping(content: &str) -> Vec<usize> {
    let mut line_mapping = vec![];
    line_mapping.push(0);
    content.match_indices("\n").for_each(|(line, _content)| {
        line_mapping.push(line + 1);
    });
    line_mapping.push(content.len() + 1);
    line_mapping
}

fn push_str_without_line(shader_content: &mut String, str: &str) {
    let mut start_index = 0;
    RE_MACRO_LINE_MULTILINE.find_iter(str).for_each(|capture| {
        shader_content.push_str(unsafe { str.get_unchecked(start_index..capture.start()) });
        start_index = capture.end();
    });
    shader_content.push_str(unsafe { str.get_unchecked(start_index..) });
}

pub fn preprocess_shader(shader_content: &mut String, pack_path: &PathBuf) {
    if let Some(capture) = RE_MACRO_VERSION.captures(&shader_content) {
        let version = capture.get(0).unwrap();
        let mut version_content = version.as_str().to_owned() + "\n";

        shader_content.replace_range(version.start()..version.end(), "");
        // If we are not in the debug folder, add Optifine's macros
        if pack_path.parent().unwrap().file_name().unwrap() != "debug" {
            version_content += OPTIFINE_MACROS;
        }
        shader_content.insert_str(0, &version_content);
    }
}

pub trait File {
    fn file_type(&self) -> &RefCell<u32>;
    fn pack_path(&self) -> &PathBuf;
    fn content(&self) -> &RefCell<String>;
    fn tree(&self) -> &RefCell<Tree>;
    fn line_mapping(&self) -> &RefCell<Vec<usize>>;
    fn included_files(&self) -> &RefCell<HashSet<PathBuf>>;
    fn including_files(&self) -> &RefCell<Vec<(usize, usize, usize, PathBuf)>>;
    fn diagnostics(&self) -> &RefCell<Vec<Diagnostic>>;

    fn including_pathes(&self) -> HashSet<PathBuf> {
        self.including_files()
            .borrow()
            .iter()
            .map(|including_data| including_data.3.clone())
            .collect::<HashSet<_>>()
    }

    fn update_from_disc(&self, parser: &mut Parser, file_path: &PathBuf) {
        if let Ok(content) = read_to_string(file_path) {
            *self.tree().borrow_mut() = parser.parse(&content, None).unwrap();
            *self.line_mapping().borrow_mut() = generate_line_mapping(&content);
            *self.content().borrow_mut() = content;
        } else {
            error!("Unable to read file {}", file_path.to_str().unwrap());
        }
    }

    fn apply_edit(&self, changes: Vec<TextDocumentContentChangeEvent>, parser: &mut Parser) {
        let mut content = self.content().borrow_mut();
        let mut tree = self.tree().borrow_mut();
        let mut line_mapping = self.line_mapping().borrow_mut();

        for change in &changes {
            let range = change.range.unwrap();
            let start_byte = line_mapping.get(range.start.line as usize).unwrap() + range.start.character as usize;
            let end_byte = line_mapping.get(range.end.line as usize).unwrap() + range.end.character as usize;

            let last_line = change.text.split("\n").enumerate().last().unwrap();
            let new_end_position = match last_line.0 {
                0 => Point {
                    row: range.start.line as usize,
                    column: range.start.character as usize + change.text.len(),
                },
                lines => Point {
                    row: range.start.line as usize + lines,
                    column: last_line.1.len(),
                },
            };
            tree.edit(&InputEdit {
                start_byte,
                old_end_byte: end_byte,
                new_end_byte: start_byte + change.text.len(),
                start_position: Point {
                    row: range.start.line as usize,
                    column: range.start.character as usize,
                },
                old_end_position: Point {
                    row: range.end.line as usize,
                    column: range.end.character as usize,
                },
                new_end_position,
            });

            content.replace_range(start_byte..end_byte, &change.text);
        }
        *tree = parser.parse(content.as_bytes(), Some(&tree)).unwrap();
        *line_mapping = generate_line_mapping(&content);
    }
}

#[derive(Clone)]
pub struct WorkspaceFile {
    /// Type of the shader
    file_type: RefCell<u32>,
    /// The shader pack path that this file in
    pack_path: PathBuf,
    /// Live content for this file
    content: RefCell<String>,
    /// Live syntax tree for this file
    tree: RefCell<Tree>,
    /// Line-content mapping
    line_mapping: RefCell<Vec<usize>>,
    /// Files that directly include this file
    included_files: RefCell<HashSet<PathBuf>>,
    /// Lines and paths for include files
    including_files: RefCell<Vec<(usize, usize, usize, PathBuf)>>,
    /// Diagnostics parsed by compiler but not tree-sitter
    diagnostics: RefCell<Vec<Diagnostic>>,
}

#[derive(Clone)]
pub struct TempFile {
    /// Type of the shader
    file_type: RefCell<u32>,
    /// The shader pack path that this file in
    pack_path: PathBuf,
    /// Live content for this file
    content: RefCell<String>,
    /// Live syntax tree for this file
    tree: RefCell<Tree>,
    /// Line-content mapping
    line_mapping: RefCell<Vec<usize>>,
    /// Files that directly include this file
    included_files: RefCell<HashSet<PathBuf>>,
    /// Lines and paths for include files
    including_files: RefCell<Vec<(usize, usize, usize, PathBuf)>>,
    /// Diagnostics parsed by compiler but not tree-sitter
    diagnostics: RefCell<Vec<Diagnostic>>,
}
