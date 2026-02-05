use once_cell::sync::OnceCell;
use tree_sitter::Query;

use super::LanguageGrammar;

pub struct CSharpGrammar;

// Static query caches for C#
static CSHARP_FUNCTIONS_QUERY: OnceCell<Query> = OnceCell::new();
static CSHARP_TYPES_QUERY: OnceCell<Query> = OnceCell::new();
static CSHARP_IMPORTS_QUERY: OnceCell<Query> = OnceCell::new();
static CSHARP_REFERENCES_QUERY: OnceCell<Query> = OnceCell::new();

impl LanguageGrammar for CSharpGrammar {
    fn name(&self) -> &'static str {
        "csharp"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["cs"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_c_sharp::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        (method_declaration
            name: (identifier) @name
            (type_parameter_list)? @type_params
            parameters: (parameter_list) @params
            type: (_)? @return_type
        ) @method

        (constructor_declaration
            name: (identifier) @name
            parameters: (parameter_list) @params
        ) @constructor

        (local_function_statement
            name: (identifier) @name
            (type_parameter_list)? @type_params
            parameters: (parameter_list) @params
            type: (_)? @return_type
        ) @function
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        (class_declaration
            name: (identifier) @name
            (type_parameter_list)? @type_params
        ) @class

        (interface_declaration
            name: (identifier) @name
            (type_parameter_list)? @type_params
        ) @interface

        (struct_declaration
            name: (identifier) @name
            (type_parameter_list)? @type_params
        ) @struct

        (enum_declaration
            name: (identifier) @name
        ) @enum

        (record_declaration
            name: (identifier) @name
            (type_parameter_list)? @type_params
        ) @record
        "#
    }

    fn imports_query(&self) -> &str {
        r#"
        (using_directive
            (identifier) @import_path
        ) @import

        (using_directive
            (qualified_name) @import_path
        ) @qualified_import
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Method calls
        (invocation_expression
            function: (identifier) @call_name
        ) @call

        (invocation_expression
            function: (member_access_expression
                name: (identifier) @method_call_name
            )
        ) @method_call

        ; Object creation
        (object_creation_expression
            type: (identifier) @constructor_call_name
        ) @constructor_call

        ; Type usages
        (identifier) @type_use

        ; Member access
        (member_access_expression
            name: (identifier) @field_access
        ) @field_expr

        ; Inheritance
        (base_list
            (identifier) @extends_type
        )
        "#
    }

    fn cached_functions_query(&self) -> Option<&'static Query> {
        CSHARP_FUNCTIONS_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.functions_query())
        }).ok()
    }

    fn cached_types_query(&self) -> Option<&'static Query> {
        CSHARP_TYPES_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.types_query())
        }).ok()
    }

    fn cached_imports_query(&self) -> Option<&'static Query> {
        CSHARP_IMPORTS_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.imports_query())
        }).ok()
    }

    fn cached_references_query(&self) -> Option<&'static Query> {
        CSHARP_REFERENCES_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.references_query())
        }).ok()
    }
}
