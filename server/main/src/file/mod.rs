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

fn generate_line_macro<I: Integer>(line: I, file_id: &str, file_name: &str) -> String {
    let mut line_macro = "#line ".to_owned();
    line_macro += Buffer::new().format(line);
    line_macro += " ";
    line_macro += file_id;
    line_macro += "\t//";
    line_macro += file_name;
    line_macro
}

fn generate_line_mapping(content: &str) -> Vec<usize> {
    let mut line_mapping = vec![];
    line_mapping.push(0);
    content.match_indices("\n").for_each(|new_line| {
        line_mapping.push(new_line.0 + 1);
    });
    line_mapping.push(content.len());
    line_mapping
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

    fn parse_includes(&self, file_path: &PathBuf) -> Vec<DocumentLink> {
        let mut include_links = vec![];

        let pack_path = self.pack_path();
        self.content().borrow().split_terminator("\n").enumerate().for_each(|line| {
            if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                let cap = capture.get(1).unwrap();
                let path = cap.as_str();

                let start = cap.start();
                let end = cap.end();

                match include_path_join(pack_path, file_path, path) {
                    Ok(include_path) => {
                        let url = Url::from_file_path(include_path).unwrap();

                        include_links.push(DocumentLink {
                            range: Range::new(Position::new(line.0 as u32, start as u32), Position::new(line.0 as u32, end as u32)),
                            tooltip: Some(url.path().to_owned()),
                            target: Some(url),
                            data: None,
                        });
                    }
                    Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
                }
            }
        });
        include_links
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

        for change in changes {
            let range = change.range.unwrap();
            let start = line_mapping.get(range.start.line as usize).unwrap() + range.start.character as usize;
            let end = line_mapping.get(range.end.line as usize).unwrap() + range.end.character as usize;

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
                start_byte: start,
                old_end_byte: end,
                new_end_byte: start + change.text.len(),
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

            content.replace_range(start..end, &change.text);
        }
        *tree = parser.parse(content.as_bytes(), Some(&tree)).unwrap();
        *line_mapping = generate_line_mapping(&content);
    }
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
}
