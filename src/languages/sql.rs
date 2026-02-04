use super::LanguageGrammar;

pub struct SqlGrammar;

impl LanguageGrammar for SqlGrammar {
    fn name(&self) -> &'static str {
        "sql"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["sql"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_sequel::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        ; Create function statements with object_reference
        (create_function
            (object_reference
                name: (identifier) @name
            )
        ) @function
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        ; Create table statements
        (create_table
            (object_reference
                name: (identifier) @name
            )
        ) @table

        ; Create view statements
        (create_view
            (object_reference
                name: (identifier) @name
            )
        ) @view

        ; Create type statements
        (create_type
            (object_reference
                name: (identifier) @name
            )
        ) @type
        "#
    }

    fn imports_query(&self) -> &str {
        ""
    }

    fn references_query(&self) -> &str {
        r#"
        ; Table/object references
        (object_reference
            name: (identifier) @ref_name
        ) @ref

        ; Field references
        (field
            (identifier) @field_name
        ) @field

        ; Function invocations
        (invocation
            (object_reference
                name: (identifier) @func_call
            )
        ) @call
        "#
    }
}
