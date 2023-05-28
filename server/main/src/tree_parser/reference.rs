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
    pub fn find_references(url: &Url, position: &Position, tree: &Tree, content: &str, line_mapping: &[usize]) -> Option<Vec<Location>> {
        let current_node = Self::current_node_fetch(position, tree, content, line_mapping)?;
        let parent = current_node.parent()?;

        match (current_node.kind(), parent.kind()) {
            (_, "function_declarator") | (_, "preproc_function_def") => {
                let query_str = function_ref_pattern(current_node.utf8_text(content.as_bytes()).unwrap());
                Some(Self::simple_global_search(url, tree, content, &query_str, line_mapping))
            }
            _ => None,
        }
    }
}
