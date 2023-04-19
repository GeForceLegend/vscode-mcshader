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
    pub static ref SYMBOLS_QUERY: Query = Query::new(tree_sitter_glsl::language(), SYMBOLS_QUERY_STR).unwrap();
}

impl TreeParser {
    #[allow(deprecated)]
    pub fn list_symbols(tree: &Tree, content: &str) -> Vec<DocumentSymbol> {
        let content_bytes = content.as_bytes();
        let mut query_cursor = QueryCursor::new();

        let mut symbols: Vec<DocumentSymbol> = vec![];
        let mut symbol_stack: LinkedList<Range> = LinkedList::new();

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
            let range = node.to_range();
            let symbol_range = match capture_name {
                "func_ident" => node.parent().unwrap().parent().unwrap().to_range(),
                _ => node.parent().unwrap().to_range(),
            };

            let name = node.utf8_text(content_bytes).unwrap().to_string();

            let curr_symbol = DocumentSymbol {
                name,
                detail: None,
                kind,
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children: None,
            };

            loop {
                if let Some(last_stack) = symbol_stack.back() {
                    if last_stack.end < range.start {
                        symbol_stack.pop_back();
                    } else {
                        let mut parent = symbols.last_mut().unwrap();
                        match &mut parent.children {
                            Some(chlidren) => chlidren.push(curr_symbol),
                            None => parent.children = Some(vec![curr_symbol; 1]),
                        }
                        break;
                    }
                } else {
                    symbols.push(curr_symbol);
                    break;
                }
            }
            symbol_stack.push_back(symbol_range);
        }
        symbols
    }
}
