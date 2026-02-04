use super::LanguageGrammar;

pub struct HclGrammar;

impl LanguageGrammar for HclGrammar {
    fn name(&self) -> &'static str {
        "hcl"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["tf", "hcl", "tfvars"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_hcl::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        ; Function calls in expressions
        (function_call
            (identifier) @name
        ) @function_call
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        ; All blocks with identifiers (resource, data, variable, output, module, etc.)
        (block
            (identifier) @block_type
            (string_lit) @name
        ) @block

        ; Blocks without string_lit labels (locals, terraform)
        (block
            (identifier) @block_type
        ) @block
        "#
    }

    fn imports_query(&self) -> &str {
        ""
    }

    fn references_query(&self) -> &str {
        r#"
        ; Variable references
        (variable_expr
            (identifier) @var_ref
        ) @var

        ; Attribute access
        (get_attr
            (identifier) @attr_ref
        ) @attr

        ; Function calls
        (function_call
            (identifier) @func_call
        ) @call

        ; Identifiers
        (identifier) @id
        "#
    }
}
