use super::LanguageGrammar;

pub struct TomlGrammar;

impl LanguageGrammar for TomlGrammar {
    fn name(&self) -> &'static str {
        "toml"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["toml"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_toml_ng::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        ""
    }

    fn types_query(&self) -> &str {
        r#"
        ; Table headers as types
        (table
            (bare_key) @name
        ) @table

        ; Array of tables
        (table_array_element
            (bare_key) @name
        ) @array_table

        ; Dotted keys
        (dotted_key
            (bare_key) @name
        ) @dotted_key
        "#
    }

    fn imports_query(&self) -> &str {
        ""
    }

    fn references_query(&self) -> &str {
        r#"
        ; Key-value pairs
        (pair
            (bare_key) @key_name
        ) @pair

        ; String values (potential references)
        (string) @string_value
        "#
    }
}
