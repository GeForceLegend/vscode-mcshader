use super::*;

fn function_ref_pattern(name: &str) -> String {
    let mut pattern = r#"(
            (call_expression
                (identifier) @call)

            (#match? @call "^"#
        .to_owned();
    pattern += name;
    pattern += r#"$"))"#;
    pattern
}

impl TreeParser {
    pub fn find_references(url: &Url, position: &Position, tree: &Tree, content: &str, line_mapping: &Vec<usize>) -> Option<Vec<Location>> {
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
            (_, "function_declarator") | (_, "preproc_function_def") => {
                let query_str = function_ref_pattern(current_node.utf8_text(content_bytes).unwrap());
                Self::simple_global_search(url, tree, content_bytes, &query_str)
            }
            _ => return None,
        };
        Some(locations)
    }
}
