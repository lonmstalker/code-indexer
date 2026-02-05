use super::LanguageGrammar;

pub struct JavaGrammar;

impl LanguageGrammar for JavaGrammar {
    fn name(&self) -> &'static str {
        "java"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["java"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        (method_declaration
            type_parameters: (type_parameters)? @type_params
            name: (identifier) @name
            parameters: (formal_parameters) @params
            type: (_)? @return_type
        ) @method

        (constructor_declaration
            type_parameters: (type_parameters)? @type_params
            name: (identifier) @name
            parameters: (formal_parameters) @params
        ) @constructor
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        (class_declaration
            name: (identifier) @name
            type_parameters: (type_parameters)? @type_params
        ) @class

        (interface_declaration
            name: (identifier) @name
            type_parameters: (type_parameters)? @type_params
        ) @interface

        (enum_declaration
            name: (identifier) @name
        ) @enum

        (record_declaration
            name: (identifier) @name
            type_parameters: (type_parameters)? @type_params
        ) @record

        (annotation_type_declaration
            name: (identifier) @name
        ) @annotation
        "#
    }

    fn imports_query(&self) -> &str {
        r#"
        (import_declaration
            (scoped_identifier) @import_path
        ) @import

        (package_declaration
            (scoped_identifier) @package_path
        ) @package
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Method invocations
        (method_invocation
            name: (identifier) @method_call_name
        ) @method_call

        ; Constructor invocations
        (object_creation_expression
            type: (type_identifier) @constructor_call_name
        ) @constructor_call

        ; Type usages
        (type_identifier) @type_use

        ; Class extension
        (class_declaration
            (superclass
                (type_identifier) @extends_type
            )
        ) @extends

        ; Interface implementation
        (class_declaration
            (super_interfaces
                (type_list
                    (type_identifier) @implements_type
                )
            )
        ) @implements

        ; Field access
        (field_access
            field: (identifier) @field_access_name
        ) @field_access

        ; Static method/field access
        (scoped_identifier
            name: (identifier) @static_access_name
        ) @static_access
        "#
    }
}
