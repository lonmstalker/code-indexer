use super::LanguageGrammar;

pub struct KotlinGrammar;

impl LanguageGrammar for KotlinGrammar {
    fn name(&self) -> &'static str {
        "kotlin"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["kt", "kts"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_kotlin_ng::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        (function_declaration
            (type_parameters)? @type_params
            name: (identifier) @name
            (function_value_parameters) @params
        ) @function

        (secondary_constructor
            (function_value_parameters) @params
        ) @constructor
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        (class_declaration
            name: (identifier) @name
            (type_parameters)? @type_params
        ) @class

        (object_declaration
            name: (identifier) @name
        ) @object

        (type_alias
            type: (identifier) @name
        ) @type_alias
        "#
    }

    fn imports_query(&self) -> &str {
        r#"
        (import
            (identifier) @import_path
        ) @import

        (import
            (qualified_identifier) @import_path
        ) @import

        (package_header
            (qualified_identifier) @package_path
        ) @package
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Function calls
        (call_expression
            (identifier) @call_name
        ) @call

        ; Method calls
        (call_expression
            (navigation_expression
                (identifier) @method_call_name
            )
        ) @method_call

        ; Constructor calls
        (call_expression
            (identifier) @constructor_call_name
        ) @constructor_call

        ; Type usages
        (user_type
            (identifier) @type_use
        ) @type_ref

        ; Class inheritance via delegation_specifiers
        (delegation_specifiers
            (delegation_specifier
                (constructor_invocation
                    (type
                        (user_type
                            (identifier) @extends_type
                        )
                    )
                )
            )
        ) @extends

        ; Class inheritance via type (interfaces)
        (delegation_specifiers
            (delegation_specifier
                (type
                    (user_type
                        (identifier) @extends_type
                    )
                )
            )
        ) @extends

        ; Property access
        (navigation_expression
            (identifier) @property_access
        ) @navigation
        "#
    }
}
