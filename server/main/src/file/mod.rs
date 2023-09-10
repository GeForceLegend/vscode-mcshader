use std::{
    cell::RefCell,
    ffi::OsString,
    fs::read_to_string,
    path::{Component, Path, PathBuf, MAIN_SEPARATOR_STR},
};

use hashbrown::{HashMap, HashSet};
use itoa::Buffer;
use logging::error;
use tower_lsp::lsp_types::*;
use tree_sitter::{InputEdit, Parser, Point, Tree};

use crate::constant::*;

mod temp_file;
mod workspace_file;

pub type IncludeInformation = (usize, usize, usize, PathBuf);

fn include_path_join(root_path: &Path, curr_path: &Path, additional: &str) -> Result<PathBuf, &'static str> {
    let mut buffer: Vec<Component>;
    let additional = match additional.strip_prefix('/') {
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
                    return Err("Unable to find parent folder while creating include path");
                }
            }
            Component::Normal(_) => buffer.push(component),
            Component::CurDir => (),
            _ => return Err("Invalid component in include path"),
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

fn push_line_macro(content: &mut String, line: usize, file_id: &str, file_name: &str) {
    content.push_str("#line ");
    content.push_str(Buffer::new().format(line));
    content.push(' ');
    content.push_str(file_id);
    content.push_str("\t//");
    content.push_str(file_name);
}

fn generate_line_mapping(content: &str) -> Vec<usize> {
    let mut line_mapping = vec![0];
    content.match_indices('\n').for_each(|(index, _)| {
        line_mapping.push(index + 1);
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

pub fn byte_index(content: &str, position: Position, line_mapping: &[usize]) -> usize {
    let line_start = line_mapping.get(position.line as usize).unwrap();
    let rest_content = unsafe { content.get_unchecked(*line_start..) };
    let line_offset = rest_content.char_indices().nth(position.character as usize).unwrap().0;
    line_start + line_offset
}

pub fn preprocess_shader(shader_content: &mut String, pack_path: &Path) -> u32 {
    if let Some(capture) = RE_MACRO_VERSION.captures(shader_content) {
        let version = capture.get(0).unwrap();
        let version_num = capture.get(1).unwrap().as_str().parse::<u32>().unwrap();
        let mut version_content = version.as_str().to_owned() + "\n";

        // If we are not in the debug folder, add Optifine's macros
        let mut components = pack_path.components();
        components.next_back();
        if components.next_back().unwrap().as_os_str() != "debug" {
            version_content += OPTIFINE_MACROS;
        }
        version_content += unsafe { shader_content.get_unchecked(..version.start()) };
        let start = version.end();

        // Since Mojang added #version in moj_import files, we must remove them so there will only one #version macro.
        let mut start_index = start;
        RE_MACRO_VERSION
            .captures_iter(unsafe { shader_content.get_unchecked(start..) })
            .for_each(|capture| {
                let version = capture.get(0).unwrap();
                let version_start = start + version.start();
                version_content += unsafe { shader_content.get_unchecked(start_index..version_start) };
                start_index = start + version.end();
            });
        version_content += unsafe { shader_content.get_unchecked(start_index..) };

        *shader_content = version_content;
        if version_num > 150 {
            1
        } else {
            2
        }
    } else {
        2
    }
}

pub trait File {
    fn file_type(&self) -> &RefCell<u32>;
    fn pack_path(&self) -> &PathBuf;
    fn content(&self) -> &RefCell<String>;
    fn tree(&self) -> &RefCell<Tree>;
    fn line_mapping(&self) -> &RefCell<Vec<usize>>;
    fn including_files(&self) -> &RefCell<Vec<IncludeInformation>>;

    fn update_from_disc(&self, parser: &mut Parser, file_path: &Path) {
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

            let start_byte = byte_index(&content, range.start, &line_mapping);
            let end_byte = byte_index(&content, range.end, &line_mapping);

            let last_line = change.text.split('\n').enumerate().last().unwrap();
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
    including_files: RefCell<Vec<IncludeInformation>>,
    /// Shaders Files that include this file
    parent_shaders: RefCell<HashSet<PathBuf>>,
    /// Diagnostics parsed by compiler but not tree-sitter
    diagnostics: RefCell<HashMap<PathBuf, Vec<Diagnostic>>>,
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
    /// Lines and paths for include files
    including_files: RefCell<Vec<IncludeInformation>>,
}
