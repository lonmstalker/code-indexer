use super::LanguageGrammar;

pub struct BashGrammar;

impl LanguageGrammar for BashGrammar {
    fn name(&self) -> &'static str {
        "bash"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["sh", "bash"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_bash::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        (function_definition
            name: (word) @name
        ) @function
        "#
    }

    fn types_query(&self) -> &str {
        ""
    }

    fn imports_query(&self) -> &str {
        r#"
        ; source command
        (command
            name: (command_name) @cmd
            (#eq? @cmd "source")
            argument: (word) @import_path
        ) @import

        ; . (dot) command for sourcing
        (command
            name: (command_name) @cmd
            (#eq? @cmd ".")
            argument: (word) @import_path
        ) @import
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Function calls (command names)
        (command
            name: (command_name
                (word) @call_name
            )
        ) @call

        ; Variable references
        (variable_name) @var_ref

        ; Expansion variables
        (expansion
            (variable_name) @var_ref
        ) @expansion
        "#
    }
}
