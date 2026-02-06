use once_cell::sync::OnceCell;
use tree_sitter::Query;

use super::LanguageGrammar;

pub struct GoGrammar;

// Static query caches for Go
static GO_FUNCTIONS_QUERY: OnceCell<Query> = OnceCell::new();
static GO_TYPES_QUERY: OnceCell<Query> = OnceCell::new();
static GO_IMPORTS_QUERY: OnceCell<Query> = OnceCell::new();
static GO_REFERENCES_QUERY: OnceCell<Query> = OnceCell::new();

impl LanguageGrammar for GoGrammar {
    fn name(&self) -> &'static str {
        "go"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["go"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_go::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        (function_declaration
            name: (identifier) @name
            type_parameters: (type_parameter_list)? @type_params
            parameters: (parameter_list) @params
            result: (_)? @return_type
        ) @function

        (method_declaration
            name: (field_identifier) @method_name
            type_parameters: (type_parameter_list)? @method_type_params
            parameters: (parameter_list) @method_params
            result: (_)? @method_return_type
        ) @method
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        (type_declaration
            (type_spec
                name: (type_identifier) @name
                type_parameters: (type_parameter_list)? @type_params
                type: (struct_type)
            )
        ) @struct

        (type_declaration
            (type_spec
                name: (type_identifier) @name
                type_parameters: (type_parameter_list)? @type_params
                type: (interface_type)
            )
        ) @interface

        (type_declaration
            (type_spec
                name: (type_identifier) @name
                type_parameters: (type_parameter_list)? @type_params
                type: (_) @alias_target
            )
        ) @type_alias
        (#not-match? @alias_target "^\\s*struct\\b")
        (#not-match? @alias_target "^\\s*interface\\b")
        "#
    }

    fn imports_query(&self) -> &str {
        r#"
        (import_declaration
            (import_spec
                path: (interpreted_string_literal) @import_path
            )
        ) @import

        (import_declaration
            (import_spec_list
                (import_spec
                    path: (interpreted_string_literal) @import_path
                )
            )
        ) @import_list
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Function calls
        (call_expression
            function: (identifier) @call_name
        ) @call

        (call_expression
            function: (selector_expression
                field: (field_identifier) @method_call_name
            )
        ) @method_call

        ; Type usages
        (type_identifier) @type_use

        ; Selector access
        (selector_expression
            field: (field_identifier) @field_access
        ) @field_expr
        "#
    }

    fn cached_functions_query(&self) -> Option<&'static Query> {
        GO_FUNCTIONS_QUERY
            .get_or_try_init(|| Query::new(&self.language(), self.functions_query()))
            .ok()
    }

    fn cached_types_query(&self) -> Option<&'static Query> {
        GO_TYPES_QUERY
            .get_or_try_init(|| Query::new(&self.language(), self.types_query()))
            .ok()
    }

    fn cached_imports_query(&self) -> Option<&'static Query> {
        GO_IMPORTS_QUERY
            .get_or_try_init(|| Query::new(&self.language(), self.imports_query()))
            .ok()
    }

    fn cached_references_query(&self) -> Option<&'static Query> {
        GO_REFERENCES_QUERY
            .get_or_try_init(|| Query::new(&self.language(), self.references_query()))
            .ok()
    }
}
