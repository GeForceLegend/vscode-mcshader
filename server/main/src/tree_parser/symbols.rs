use super::*;

const SYMBOLS_QUERY_STR: &str = r#"
    ; global consts
    (declaration
        (type_qualifier) @const_qualifier
            (init_declarator
                (identifier) @const_ident))
    (#match? @const_qualifier "^const")

    ; global uniforms, varyings, struct variables etc
    (translation_unit
    	(declaration
    		(identifier) @ident))

    ; #defines
    (preproc_def
        (identifier) @define_ident)

    ; function definitions
    (function_declarator
        (identifier) @func_ident)

    ; inline functions
    (preproc_function_def
        (identifier) @preproc_func_ident)

    ; struct definitions
    (struct_specifier
        (type_identifier) @struct_ident)

    ; struct fields
    (struct_specifier
        (field_declaration_list
            (field_declaration
                [
                  (field_identifier) @field_ident
                  (array_declarator
                      (field_identifier) @field_ident)
                 ])) @field_list)
"#;

lazy_static! {
    static ref SYMBOLS_QUERY: Query = Query::new(tree_sitter_glsl::language(), SYMBOLS_QUERY_STR).unwrap();
}

// This does not need unsafe code to create another reference
fn insert_child_symbol(parent_list: &mut Vec<DocumentSymbol>, symbol: DocumentSymbol) {
    let possible_parent = parent_list.last_mut().unwrap();
    if possible_parent.range.end < symbol.range.end {
        parent_list.push(symbol);
    } else if let Some(children_list) = &mut possible_parent.children {
        insert_child_symbol(children_list, symbol);
    } else {
        possible_parent.children = Some(vec![symbol; 1]);
    }
}

impl TreeParser {
    pub fn list_symbols(tree: &Tree, content: &str, line_mapping: &[usize]) -> Vec<DocumentSymbol> {
        let content_bytes = content.as_bytes();
        let mut query_cursor = QueryCursor::new();

        let mut symbols: Vec<DocumentSymbol> = vec![];

        for query_match in query_cursor.matches(&SYMBOLS_QUERY, tree.root_node(), content_bytes) {
            let mut capture_iter = query_match.captures.iter();
            let capture = match capture_iter.next() {
                Some(capture) => capture,
                None => continue,
            };

            let capture_name = SYMBOLS_QUERY.capture_names()[capture.index as usize].as_str();

            let (kind, node) = match capture_name {
                "const_qualifier" => (SymbolKind::CONSTANT, capture_iter.next().unwrap().node),
                "ident" => (SymbolKind::VARIABLE, capture.node),
                "preproc_func_ident" => (SymbolKind::FUNCTION, capture.node),
                "func_ident" => (SymbolKind::FUNCTION, capture.node),
                "define_ident" => (SymbolKind::STRING, capture.node),
                "struct_ident" => (SymbolKind::STRUCT, capture.node),
                "field_list" => (SymbolKind::FIELD, capture_iter.next().unwrap().node),
                _ => (SymbolKind::NULL, capture.node),
            };
            let selection_range = node.to_range(content, line_mapping);
            let range = match capture_name {
                "func_ident" => node.parent().unwrap().parent().unwrap().to_range(content, line_mapping),
                _ => node.parent().unwrap().to_range(content, line_mapping),
            };
            let name = node.utf8_text(content_bytes).unwrap().to_string();
            if name.is_empty() {
                continue;
            }

            #[allow(deprecated)]
            let current_symbol = DocumentSymbol {
                name,
                detail: None,
                kind,
                tags: None,
                deprecated: None,
                range,
                selection_range,
                children: None,
            };

            if symbols.is_empty() {
                symbols.push(current_symbol);
            } else {
                insert_child_symbol(&mut symbols, current_symbol);
            }
        }
        symbols
    }
}
