use super::LanguageGrammar;

pub struct LuaGrammar;

impl LanguageGrammar for LuaGrammar {
    fn name(&self) -> &'static str {
        "lua"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["lua"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_lua::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        ; Global function declarations
        (function_declaration
            name: (identifier) @name
        ) @function

        ; Method definitions via dot (table.method)
        (function_declaration
            name: (dot_index_expression
                field: (identifier) @name
            )
        ) @method

        ; Method definitions via colon (table:method)
        (function_declaration
            name: (method_index_expression
                method: (identifier) @name
            )
        ) @method
        "#
    }

    fn types_query(&self) -> &str {
        ""
    }

    fn imports_query(&self) -> &str {
        r#"
        ; require calls
        (function_call
            name: (identifier) @fn_name
            (#eq? @fn_name "require")
            arguments: (arguments
                (string) @import_path
            )
        ) @import

        ; dofile calls
        (function_call
            name: (identifier) @fn_name
            (#eq? @fn_name "dofile")
            arguments: (arguments
                (string) @import_path
            )
        ) @import

        ; loadfile calls
        (function_call
            name: (identifier) @fn_name
            (#eq? @fn_name "loadfile")
            arguments: (arguments
                (string) @import_path
            )
        ) @import
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Function calls
        (function_call
            name: (identifier) @call_name
        ) @call

        ; Method calls
        (function_call
            name: (method_index_expression
                method: (identifier) @method_call
            )
        ) @method_call

        ; Table field access
        (dot_index_expression
            field: (identifier) @field_access
        ) @access

        ; Variable references
        (identifier) @var_ref
        "#
    }
}
