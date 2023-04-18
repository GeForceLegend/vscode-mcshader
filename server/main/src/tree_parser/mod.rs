mod definition;
mod reference;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Location, Position, Range};
use tree_sitter::{Node, Query, QueryCursor, Tree, TreeCursor};
use url::Url;

trait ToLspTypes {
    fn to_location(&self, url: &Url) -> Location;
    fn to_range(&self) -> Range;
}

impl ToLspTypes for Node<'_> {
    fn to_location(&self, url: &Url) -> Location {
        let start = self.start_position();
        let end = self.end_position();
        Location {
            uri: url.clone(),
            range: Range {
                start: Position {
                    line: start.row as u32,
                    character: start.column as u32,
                },
                end: Position {
                    line: end.row as u32,
                    character: end.column as u32,
                },
            },
        }
    }

    fn to_range(&self) -> Range {
        let start = self.start_position();
        let end = self.end_position();
        Range {
            start: Position {
                line: start.row as u32,
                character: start.column as u32,
            },
            end: Position {
                line: end.row as u32,
                character: end.column as u32,
            },
        }
    }
}

pub struct TreeParser;

impl TreeParser {
    fn current_node_fetch<'a>(position: &Position, tree: &'a Tree, content: &[u8], line_mapping: &Vec<usize>) -> Option<Node<'a>> {
        let position_offset = line_mapping[position.line as usize] + position.character as usize;

        match content[position_offset] {
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'_' => tree
                .root_node()
                .named_descendant_for_byte_range(position_offset, position_offset + 1),
            _ => tree
                .root_node()
                .named_descendant_for_byte_range(position_offset - 1, position_offset),
        }
    }

    pub fn error_search(cursor: &mut TreeCursor, error_list: &mut Vec<Diagnostic>) {
        loop {
            let current_node = cursor.node();
            if current_node.is_error() {
                error_list.push(Diagnostic {
                    range: current_node.to_range(),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: None,
                    code_description: None,
                    source: Some("mcshader-glsl".to_owned()),
                    message: "Syntax error by simple real-time search".to_owned(),
                    related_information: None,
                    tags: None,
                    data: None,
                });
            } else if cursor.goto_first_child() {
                Self::error_search(cursor, error_list);
                cursor.goto_parent();
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    pub fn simple_lint(tree: &Tree) -> Vec<Diagnostic> {
        let mut cursor = tree.walk();

        let mut error_list = vec![];
        Self::error_search(&mut cursor, &mut error_list);
        error_list
    }

    fn simple_global_search(url: &Url, tree: &Tree, content: &[u8], query_str: &str) -> Vec<Location> {
        let query = Query::new(tree_sitter_glsl::language(), query_str).unwrap();
        let mut query_cursor = QueryCursor::new();

        let mut locations = vec![];

        for m in query_cursor.matches(&query, tree.root_node(), content) {
            locations.extend(m.captures.iter().map(|capture| capture.node.to_location(url)));
        }

        locations
    }
}
