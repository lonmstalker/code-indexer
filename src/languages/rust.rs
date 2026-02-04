use once_cell::sync::OnceCell;
use tree_sitter::Query;

use super::LanguageGrammar;

pub struct RustGrammar;

// Static query caches for Rust
static RUST_FUNCTIONS_QUERY: OnceCell<Query> = OnceCell::new();
static RUST_TYPES_QUERY: OnceCell<Query> = OnceCell::new();
static RUST_IMPORTS_QUERY: OnceCell<Query> = OnceCell::new();
static RUST_REFERENCES_QUERY: OnceCell<Query> = OnceCell::new();

impl LanguageGrammar for RustGrammar {
    fn name(&self) -> &'static str {
        "rust"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["rs"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        (function_item
            name: (identifier) @name
            parameters: (parameters) @params
            return_type: (_)? @return_type
        ) @function

        (impl_item
            type: (_) @impl_type
            body: (declaration_list
                (function_item
                    name: (identifier) @method_name
                    parameters: (parameters) @method_params
                    return_type: (_)? @method_return_type
                ) @method
            )
        )
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        (struct_item
            name: (type_identifier) @name
        ) @struct

        (enum_item
            name: (type_identifier) @name
        ) @enum

        (trait_item
            name: (type_identifier) @name
        ) @trait

        (type_item
            name: (type_identifier) @name
        ) @type_alias

        (impl_item
            trait: (type_identifier)? @trait_name
            type: (_) @impl_type
        ) @impl
        "#
    }

    fn imports_query(&self) -> &str {
        r#"
        (use_declaration
            argument: (_) @import_path
        ) @import

        (extern_crate_declaration
            name: (identifier) @crate_name
        ) @extern_crate
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Function/method calls
        (call_expression
            function: (identifier) @call_name
        ) @call

        (call_expression
            function: (field_expression
                field: (field_identifier) @method_call_name
            )
        ) @method_call

        (call_expression
            function: (scoped_identifier
                name: (identifier) @scoped_call_name
            )
        ) @scoped_call

        ; Type usages
        (type_identifier) @type_use

        ; Trait implementations
        (impl_item
            trait: (type_identifier) @impl_trait
            type: (type_identifier) @impl_type
        ) @impl_for_trait

        ; Field access
        (field_expression
            field: (field_identifier) @field_access
        ) @field_expr

        ; Macro invocations
        (macro_invocation
            macro: (identifier) @macro_name
        ) @macro_call
        "#
    }

    fn cached_functions_query(&self) -> Option<&'static Query> {
        RUST_FUNCTIONS_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.functions_query())
        }).ok()
    }

    fn cached_types_query(&self) -> Option<&'static Query> {
        RUST_TYPES_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.types_query())
        }).ok()
    }

    fn cached_imports_query(&self) -> Option<&'static Query> {
        RUST_IMPORTS_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.imports_query())
        }).ok()
    }

    fn cached_references_query(&self) -> Option<&'static Query> {
        RUST_REFERENCES_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.references_query())
        }).ok()
    }
}
