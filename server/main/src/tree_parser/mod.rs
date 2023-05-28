use lazy_static::lazy_static;
use tower_lsp::lsp_types::*;
use tree_sitter::{Node, Query, QueryCursor, Tree, TreeCursor};
use url::Url;

use crate::file::byte_index;

mod definition;
mod reference;
mod simple_lint;
mod symbols;

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
    fn current_node_fetch<'a>(position: &Position, tree: &'a Tree, content: &str, line_mapping: &[usize]) -> Option<Node<'a>> {
        let position_offset = byte_index(content, *position, line_mapping);

        let (start, end) = match content.as_bytes()[position_offset] {
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'_' => (position_offset, position_offset + 1),
            _ => (position_offset - 1, position_offset),
        };
        tree.root_node().named_descendant_for_byte_range(start, end)
    }

    fn simple_global_search(url: &Url, tree: &Tree, content: &[u8], query_str: &str) -> Vec<Location> {
        let query = Query::new(tree_sitter_glsl::language(), query_str).unwrap();
        let mut query_cursor = QueryCursor::new();

        let mut locations = vec![];

        for query_match in query_cursor.matches(&query, tree.root_node(), content) {
            locations.extend(query_match.captures.iter().map(|capture| capture.node.to_location(url)));
        }

        locations
    }
}
