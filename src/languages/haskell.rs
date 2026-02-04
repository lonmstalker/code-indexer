use super::LanguageGrammar;

pub struct HaskellGrammar;

impl LanguageGrammar for HaskellGrammar {
    fn name(&self) -> &'static str {
        "haskell"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["hs", "lhs"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_haskell::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        ; Function definitions with variable name
        (function
            name: (variable) @name
        ) @function

        ; Function definitions with prefix_id
        (function
            name: (prefix_id) @name
        ) @function

        ; Top-level bindings with variable name
        (bind
            name: (variable) @name
        ) @binding

        ; Signature declarations with variable name
        (signature
            name: (variable) @name
        ) @signature
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        ; Data type declarations with name
        (data_type
            name: (name) @name
        ) @data

        ; Data type declarations with prefix_id
        (data_type
            name: (prefix_id) @name
        ) @data

        ; Newtype declarations with name
        (newtype
            name: (name) @name
        ) @newtype

        ; Newtype declarations with prefix_id
        (newtype
            name: (prefix_id) @name
        ) @newtype

        ; Type synonyms (type aliases) with name
        (type_synomym
            name: (name) @name
        ) @type_alias

        ; Type synonyms with prefix_id
        (type_synomym
            name: (prefix_id) @name
        ) @type_alias

        ; Type class declarations
        (class_decl) @class

        ; Type instances
        (instance_decl) @instance
        "#
    }

    fn imports_query(&self) -> &str {
        r#"
        ; Import statements
        (import
            module: (module) @import_path
        ) @import
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Variable references
        (variable) @var_ref

        ; Constructor references
        (constructor) @constructor_ref

        ; Qualified names
        (qualified) @qualified

        ; Operator references
        (operator) @op_ref
        "#
    }
}
