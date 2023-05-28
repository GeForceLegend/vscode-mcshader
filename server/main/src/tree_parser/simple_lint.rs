use super::*;

impl TreeParser {
    pub fn error_search(content: &str, line_mapping: &[usize], cursor: &mut TreeCursor, error_list: &mut Vec<Diagnostic>) {
        loop {
            let current_node = cursor.node();
            if current_node.is_error() {
                error_list.push(Diagnostic {
                    range: current_node.to_range(content, line_mapping),
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("mcshader-glsl".to_owned()),
                    message: "Syntax error by simple real-time search".to_owned(),
                    ..Default::default()
                });
            } else if cursor.goto_first_child() {
                Self::error_search(content, line_mapping, cursor, error_list);
                cursor.goto_parent();
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    pub fn simple_lint(tree: &Tree, content: &str, line_mapping: &[usize]) -> Vec<Diagnostic> {
        let mut cursor = tree.walk();

        let mut error_list = vec![];
        Self::error_search(content, line_mapping, &mut cursor, &mut error_list);
        error_list
    }
}
