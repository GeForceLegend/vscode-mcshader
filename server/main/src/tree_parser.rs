use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{Location, Position, Range};
use tree_sitter::{Node, Point, Query, QueryCursor, Tree};
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

pub struct TreeParser {}

impl TreeParser {
    fn generate_line_mapping(content: &String) -> Vec<usize> {
        let mut line_mapping: Vec<usize> = vec![0];
        for (i, char) in content.char_indices() {
            if char == '\n' {
                line_mapping.push(i + 1);
            }
        }
        line_mapping
    }

    pub fn find_definitions(url: &Url, position: &Position, tree: &Tree, content: &String) -> Result<Option<Vec<Location>>> {
        let line_mapping = Self::generate_line_mapping(&content);
        let position_offset = line_mapping[position.line as usize] + position.character as usize;

        let mut start = Point {
            row: position.line as usize,
            column: position.character as usize,
        };
        let mut end = Point {
            row: position.line as usize,
            column: position.character as usize,
        };
        if content.as_bytes()[position_offset].is_ascii_alphanumeric() {
            end.column += 1;
        } else {
            start.column -= 1;
        }

        let current_node = match tree.root_node().named_descendant_for_point_range(start, end) {
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
            },
            (_, "function_declarator") | (_, "preproc_function_def") => {
                let start = current_node.start_position();
                let end = current_node.end_position();

                vec![Location {
                    uri: url.to_owned(),
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
                }]
            },
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
        let line_mapping = Self::generate_line_mapping(&content);
        let position_offset = line_mapping[position.line as usize] + position.character as usize;

        let mut start = Point {
            row: position.line as usize,
            column: position.character as usize,
        };
        let mut end = Point {
            row: position.line as usize,
            column: position.character as usize,
        };
        if content.as_bytes()[position_offset].is_ascii_alphanumeric() {
            end.column += 1;
        } else {
            start.column -= 1;
        }

        let current_node = match tree.root_node().named_descendant_for_point_range(start, end) {
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
                let start = capture.node.start_position();
                let end = capture.node.end_position();

                locations.push(Location {
                    uri: url.to_owned(),
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
                });
            }
        }

        locations
    }

    fn tree_climbing_search(source: &String, url: &Url, start_node: Node) -> Vec<Location> {
        let mut locations = vec![];

        let node_text = start_node.utf8_text(source.as_bytes()).unwrap();
        let node_pos = start_node.start_position();
        let query_str = format!(find_variable_def_str!(), node_text);

        let mut parent = start_node.parent();

        loop {
            if parent.is_none() {
                break;
            }

            let query = Query::new(tree_sitter_glsl::language(), &query_str).unwrap();
            let mut query_cursor = QueryCursor::new();

            for m in query_cursor.matches(&query, parent.unwrap(), source.as_bytes()) {
                for capture in m.captures {
                    let start = capture.node.start_position();
                    let end = capture.node.end_position();

                    if start.row > node_pos.row || (start.row == node_pos.row && start.column > node_pos.column) {
                        continue;
                    }

                    locations.push(Location {
                        uri: url.to_owned(),
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
                    });
                }
            }

            if !locations.is_empty() {
                break;
            }

            parent = parent.unwrap().parent();
        }

        locations
    }
}
