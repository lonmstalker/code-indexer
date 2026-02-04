use super::LanguageGrammar;

pub struct YamlGrammar;

impl LanguageGrammar for YamlGrammar {
    fn name(&self) -> &'static str {
        "yaml"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["yml", "yaml"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_yaml::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        ""
    }

    fn types_query(&self) -> &str {
        r#"
        ; Top-level keys as types/sections
        (block_mapping_pair
            key: (flow_node
                (plain_scalar
                    (string_scalar) @name
                )
            )
        ) @key

        ; Anchor definitions
        (anchor
            (anchor_name) @name
        ) @anchor
        "#
    }

    fn imports_query(&self) -> &str {
        ""
    }

    fn references_query(&self) -> &str {
        r#"
        ; Alias references
        (alias
            (alias_name) @alias_ref
        ) @ref

        ; Scalar values that might be references
        (plain_scalar
            (string_scalar) @value
        ) @scalar
        "#
    }
}
