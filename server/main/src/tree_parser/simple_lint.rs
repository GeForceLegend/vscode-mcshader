use super::*;

impl TreeParser {
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
            } else if current_node.has_error() && cursor.goto_first_child() {
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
}
