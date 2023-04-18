use super::*;

fn function_def_pattern(name: &str) -> String {
    let mut pattern = r#"[
            (function_declarator 
                (identifier) @function)

            (preproc_function_def name: 
                (identifier) @function)

            (#match? @function "^"#
        .to_owned();
    pattern += name;
    pattern += r#"$")]"#;
    pattern
}

fn variable_def_pattern(name: &str) -> String {
    let mut pattern = r#"[
            (init_declarator
                declarator: (identifier) @variable)

            (parameter_declaration
                declarator: (identifier) @variable)

            (declaration
                declarator: (identifier) @variable)

            (preproc_def
                name: (identifier) @variable)

            (#match? @variable "^"#
        .to_owned();
    pattern += name;
    pattern += r#"$")]"#;
    pattern
}

impl TreeParser {
    fn tree_climbing_search(source: &[u8], url: &Url, start_node: Node) -> Vec<Location> {
        let mut locations = vec![];

        let node_text = start_node.utf8_text(source).unwrap();
        let query_str = variable_def_pattern(node_text);

        let mut parent = start_node.parent();

        let query = Query::new(tree_sitter_glsl::language(), &query_str).unwrap();
        let mut query_cursor = QueryCursor::new();
        query_cursor.set_byte_range(0..start_node.end_byte());

        while let Some(parent_node) = parent {
            for m in query_cursor.matches(&query, parent_node, source) {
                locations.extend(m.captures.iter().map(|capture| capture.node.to_location(url)));
            }

            if !locations.is_empty() {
                break;
            }

            parent = parent_node.parent();
        }

        locations
    }

    pub fn find_definitions(
        url: &Url, position: &Position, tree: &Tree, content: &str, line_mapping: &Vec<usize>,
    ) -> Option<Vec<Location>> {
        let content_bytes = content.as_bytes();
        let current_node = match Self::current_node_fetch(position, tree, content_bytes, line_mapping) {
            Some(node) => node,
            None => return None,
        };

        let parent = match current_node.parent() {
            Some(parent) => parent,
            None => return None,
        };

        let locations = match (current_node.kind(), parent.kind()) {
            (_, "call_expression") => {
                let query_str = function_def_pattern(current_node.utf8_text(content_bytes).unwrap());
                Self::simple_global_search(url, tree, content_bytes, &query_str)
            }
            (_, "function_declarator") | (_, "preproc_function_def") => vec![current_node.to_location(url); 1],
            ("identifier", "argument_list")
            | ("identifier", "field_expression")
            | ("identifier", "binary_expression")
            | ("identifier", "return_statement")
            | ("identifier", "assignment_expression") => Self::tree_climbing_search(content_bytes, url, current_node),
            ("identifier", "init_declarator") => match current_node.prev_sibling() {
                Some(_) => Self::tree_climbing_search(content_bytes, url, current_node),
                None => vec![],
            },
            _ => return None,
        };
        Some(locations)
    }
}
