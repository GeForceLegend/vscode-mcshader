use std::{
    cell::RefCell,
    collections::HashSet,
    ffi::OsString,
    path::{Component, PathBuf, MAIN_SEPARATOR_STR},
};

use logging::error;
use tower_lsp::lsp_types::*;
use tree_sitter::{InputEdit, Parser, Point, Tree};

use crate::constant::RE_MACRO_INCLUDE;

mod include_file;
mod shader_file;
mod temp_file;

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
                    return Err("Unable to find parent while creating include path".into());
                }
            }
            Component::Normal(_) => buffer.push(component),
            Component::CurDir => {}
            _ => return Err("Invalid component in include path".into()),
        }
    }

    let mut resource = OsString::new();
    let last = buffer.pop().unwrap();
    for component in buffer {
        resource.push(component);
        resource.push(MAIN_SEPARATOR_STR);
    }
    resource.push(last);

    Ok(PathBuf::from(resource))
}

pub trait File {
    fn pack_path(&self) -> &PathBuf;
    fn content(&self) -> &RefCell<String>;
    fn tree(&self) -> &RefCell<Tree>;

    fn parse_includes(&self, file_path: &PathBuf) -> Vec<DocumentLink> {
        let mut include_links = Vec::new();

        let pack_path = self.pack_path();
        self.content().borrow().lines().enumerate().for_each(|line| {
            if let Some(capture) = RE_MACRO_INCLUDE.captures(line.1) {
                let cap = capture.get(1).unwrap();
                let path = cap.as_str();

                let start = cap.start();
                let end = cap.end();

                match include_path_join(pack_path, file_path, path) {
                    Ok(include_path) => {
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
                    Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
                }
            }
        });
        include_links
    }

    fn generate_line_mapping(&self) -> Vec<usize> {
        let mut line_mapping: Vec<usize> = vec![0];
        for (i, char) in self.content().borrow().char_indices() {
            if char == '\n' {
                line_mapping.push(i + 1);
            }
        }
        line_mapping
    }

    fn apply_edit(&self, changes: Vec<TextDocumentContentChangeEvent>, parser: &mut Parser) {
        let line_mapping = self.generate_line_mapping();

        let mut content = self.content().borrow_mut();
        let mut tree = self.tree().borrow_mut();

        changes.iter().for_each(|change| {
            let range = change.range.unwrap();
            let start = line_mapping.get(range.start.line as usize).unwrap() + range.start.character as usize;
            let end = line_mapping.get(range.end.line as usize).unwrap() + range.end.character as usize;

            let new_end_position = match change.text.matches("\n").count() {
                0 => Point {
                    row: range.start.line as usize,
                    column: range.start.character as usize + change.text.len(),
                },
                lines => Point {
                    row: range.start.line as usize + lines - content.get(start..end).unwrap().matches("\n").count(),
                    column: change.text.split("\n").last().unwrap().len(),
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
        });
        *tree = parser.parse(content.as_bytes(), Some(&tree)).unwrap();
    }
}

pub trait BaseShader: File {
    fn file_type(&self) -> gl::types::GLenum;
    // fn full_content(&self) -> &RefCell<String>;
    // fn full_tree(&self) -> &RefCell<Tree>;
}

#[derive(Clone)]
pub struct ShaderFile {
    /// Type of the shader
    file_type: gl::types::GLenum,
    /// The shader pack path that this file in
    pack_path: PathBuf,
    /// Live content for this file
    content: RefCell<String>,
    /// Live syntax tree for this file
    tree: RefCell<Tree>,
}

#[derive(Clone)]
pub struct IncludeFile {
    /// The shader pack path that this file in
    pack_path: PathBuf,
    /// Live content for this file
    content: RefCell<String>,
    /// Live syntax tree for this file
    tree: RefCell<Tree>,
    /// Shader files that include this file
    included_shaders: RefCell<HashSet<PathBuf>>,
    /// Files included in this file
    /// Though we can scan its content and get includes,
    /// keep a collection helps update parents faster
    including_files: RefCell<HashSet<PathBuf>>,
}

#[derive(Clone)]
pub struct TempFile {
    /// Type of the shader
    file_type: gl::types::GLenum,
    /// The shader pack path that this file in
    pack_path: PathBuf,
    /// Live content for this file
    content: RefCell<String>,
    /// Live syntax tree for this file
    tree: RefCell<Tree>,
}
