use super::LanguageGrammar;

pub struct TypeScriptGrammar;

impl LanguageGrammar for TypeScriptGrammar {
    fn name(&self) -> &'static str {
        "typescript"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["ts", "tsx", "js", "jsx"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        (function_declaration
            name: (identifier) @name
            type_parameters: (type_parameters)? @type_params
            parameters: (formal_parameters) @params
            return_type: (_)? @return_type
        ) @function

        (method_definition
            name: (property_identifier) @name
            type_parameters: (type_parameters)? @type_params
            parameters: (formal_parameters) @params
            return_type: (_)? @return_type
        ) @method

        (arrow_function
            parameter: (identifier) @param
        ) @arrow_function

        (arrow_function
            type_parameters: (type_parameters)? @type_params
            parameters: (formal_parameters) @params
            return_type: (_)? @return_type
        ) @arrow_function_multi

        (variable_declarator
            name: (identifier) @name
            value: (arrow_function) @arrow
        ) @named_arrow
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        (class_declaration
            name: (type_identifier) @name
            type_parameters: (type_parameters)? @type_params
        ) @class

        (interface_declaration
            name: (type_identifier) @name
            type_parameters: (type_parameters)? @type_params
        ) @interface

        (type_alias_declaration
            name: (type_identifier) @name
            type_parameters: (type_parameters)? @type_params
        ) @type_alias

        (enum_declaration
            name: (identifier) @name
        ) @enum
        "#
    }

    fn imports_query(&self) -> &str {
        r#"
        (import_statement
            source: (string) @source
        ) @import

        (export_statement
            source: (string)? @export_source
        ) @export
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Function/method calls
        (call_expression
            function: (identifier) @call_name
        ) @call

        (call_expression
            function: (member_expression
                property: (property_identifier) @method_call_name
            )
        ) @method_call

        ; Constructor calls
        (new_expression
            constructor: (identifier) @constructor_call_name
        ) @constructor_call

        ; Type annotations
        (type_identifier) @type_use

        ; Class extension
        (class_heritage
            (extends_clause
                (identifier) @extends_type
            )
        ) @extends

        ; Interface implementation
        (class_heritage
            (implements_clause
                (type_identifier) @implements_type
            )
        ) @implements

        ; Property access
        (member_expression
            property: (property_identifier) @property_access
        ) @member_access
        "#
    }
}
