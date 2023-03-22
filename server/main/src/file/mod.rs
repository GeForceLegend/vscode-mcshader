use std::{
    collections::HashSet,
    path::PathBuf,
    cell::RefCell,
};

use path_slash::PathBufExt;
use tower_lsp::lsp_types::*;
use tree_sitter::{Tree, InputEdit, Point, Parser};

use crate::constant::RE_MACRO_INCLUDE;

mod include_file;
mod shader_file;
mod temp_file;

pub trait File {
    fn pack_path(&self) -> &PathBuf;
    fn content(&self) -> &RefCell<String>;
    fn tree(&self) -> &RefCell<Tree>;

    fn parse_includes(&self, file_path: &PathBuf) -> Vec<DocumentLink> {
        let mut include_links = Vec::new();

        let pack_path = self.pack_path();
        self.content().borrow().lines()
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
        let mut content = self.content().borrow_mut();
        let mut tree = self.tree().borrow_mut();

        let line_mapping = self.generate_line_mapping();

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
