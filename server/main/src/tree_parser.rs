use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{Location, Position, Range};
use tree_sitter::{Node, Query, QueryCursor, Tree};
use url::Url;

macro_rules! find_function_def_str {
    () => {
        r#"
            [
                (function_declarator 
                    (identifier) @function)

                (preproc_function_def 
                    name: (identifier) @function)

                (#match? @function "^{}$")
            ]
        "#
    };
}

macro_rules! find_function_refs_str {
    () => {
        r#"
            (
                (call_expression 
                    (identifier) @call)
                (#match? @call "^{}$")
            )
        "#
    };
}

macro_rules! find_variable_def_str {
    () => {
        r#"
            [
                (init_declarator 
                    declarator: (identifier) @variable)

                (parameter_declaration
                    declarator: (identifier) @variable)

                (declaration
                    declarator: (identifier) @variable)

                (preproc_def
                    name: (identifier) @variable)

                (#match? @variable "^{}$")
            ]
        "#
    };
}

trait ToRange {
    fn to_range(&self) -> Range;
}

impl ToRange for Node<'_> {
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

pub struct TreeParser {}

impl TreeParser {
    fn generate_line_mapping(content: &String) -> Vec<usize> {
        let mut line_mapping: Vec<usize> = std::vec::from_elem(0, 1);
        content.match_indices("\n").for_each(|new_line| {
            line_mapping.push(new_line.0 + 1);
        });
        line_mapping
    }

    fn current_node_fetch<'a>(position: &'a Position, tree: &'a Tree, content: &'a String) -> Option<Node<'a>> {
        let line_mapping = Self::generate_line_mapping(content);
        let position_offset = line_mapping[position.line as usize] + position.character as usize;

        if content.as_bytes()[position_offset].is_ascii_alphanumeric() {
            tree.root_node().named_descendant_for_byte_range(position_offset, position_offset + 1)
        } else {
            tree.root_node().named_descendant_for_byte_range(position_offset - 1, position_offset)
        }
    }

    pub fn find_definitions(url: &Url, position: &Position, tree: &Tree, content: &String) -> Result<Option<Vec<Location>>> {
        let current_node = match Self::current_node_fetch(position, tree, content) {
            Some(node) => node,
            None => return Ok(None),
        };

        let parent = match current_node.parent() {
            Some(parent) => parent,
            None => return Ok(None),
        };

        let locations = match (current_node.kind(), parent.kind()) {
            (_, "call_expression") => {
                let query_str = format!(find_function_def_str!(), current_node.utf8_text(content.as_bytes()).unwrap());
                Self::simple_global_search(url, tree, content, &query_str)
            }
            (_, "function_declarator") | (_, "preproc_function_def") => {
                vec![Location {
                    uri: url.to_owned(),
                    range: current_node.to_range(),
                }]
            }
            ("identifier", "argument_list")
            | ("identifier", "field_expression")
            | ("identifier", "binary_expression")
            | ("identifier", "return_statement")
            | ("identifier", "assignment_expression") => Self::tree_climbing_search(&content, url, current_node),
            ("identifier", "init_declarator") => match current_node.prev_sibling() {
                Some(_) => Self::tree_climbing_search(&content, url, current_node),
                None => Vec::new(),
            },
            _ => return Ok(None),
        };
        Ok(Some(locations))
    }

    pub fn find_references(url: &Url, position: &Position, tree: &Tree, content: &String) -> Result<Option<Vec<Location>>> {
        let current_node = match Self::current_node_fetch(position, tree, content) {
            Some(node) => node,
            None => return Ok(None),
        };

        let parent = match current_node.parent() {
            Some(parent) => parent,
            None => return Ok(None),
        };

        let locations = match (current_node.kind(), parent.kind()) {
            (_, "function_declarator") | (_, "preproc_function_def") => {
                let query_str = format!(find_function_refs_str!(), current_node.utf8_text(content.as_bytes()).unwrap());
                Self::simple_global_search(url, tree, content, &query_str)
            }
            _ => return Ok(None),
        };
        Ok(Some(locations))
    }

    fn simple_global_search(url: &Url, tree: &Tree, content: &String, query_str: &str) -> Vec<Location> {
        let query = Query::new(tree_sitter_glsl::language(), query_str).unwrap();
        let mut query_cursor = QueryCursor::new();

        let mut locations = vec![];

        for m in query_cursor.matches(&query, tree.root_node(), content.as_bytes()) {
            for capture in m.captures {
                locations.push(Location {
                    uri: url.to_owned(),
                    range: capture.node.to_range(),
                });
            }
        }

        locations
    }

    fn tree_climbing_search(source: &String, url: &Url, start_node: Node) -> Vec<Location> {
        let mut locations = vec![];

        let node_text = start_node.utf8_text(source.as_bytes()).unwrap();
        let query_str = format!(find_variable_def_str!(), node_text);

        let mut parent = start_node.parent();

        let query = Query::new(tree_sitter_glsl::language(), &query_str).unwrap();
        let mut query_cursor = QueryCursor::new();
        query_cursor.set_byte_range(0..start_node.end_byte());
        let text_provider = source.as_bytes();

        while let Some(parent_node) = parent {
            for m in query_cursor.matches(&query, parent_node, text_provider) {
                for capture in m.captures {
                    locations.push(Location {
                        uri: url.to_owned(),
                        range: capture.node.to_range(),
                    });
                }
            }

            if !locations.is_empty() {
                break;
            }

            parent = parent_node.parent();
        }

        locations
    }
}
