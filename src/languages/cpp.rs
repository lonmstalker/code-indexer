use once_cell::sync::OnceCell;
use tree_sitter::Query;

use super::LanguageGrammar;

pub struct CppGrammar;

// Static query caches for C++
static CPP_FUNCTIONS_QUERY: OnceCell<Query> = OnceCell::new();
static CPP_TYPES_QUERY: OnceCell<Query> = OnceCell::new();
static CPP_IMPORTS_QUERY: OnceCell<Query> = OnceCell::new();
static CPP_REFERENCES_QUERY: OnceCell<Query> = OnceCell::new();

impl LanguageGrammar for CppGrammar {
    fn name(&self) -> &'static str {
        "cpp"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["cpp", "cc", "cxx", "hpp", "h", "hxx"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_cpp::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        (function_definition
            declarator: (function_declarator
                declarator: (identifier) @name
                parameters: (parameter_list) @params
            )
            type: (_)? @return_type
        ) @function

        (function_definition
            declarator: (function_declarator
                declarator: (qualified_identifier
                    name: (identifier) @method_name
                )
                parameters: (parameter_list) @method_params
            )
            type: (_)? @method_return_type
        ) @method

        (declaration
            declarator: (function_declarator
                declarator: (identifier) @name
                parameters: (parameter_list) @params
            )
            type: (_)? @return_type
        ) @function_decl
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        (struct_specifier
            name: (type_identifier) @name
        ) @struct

        (class_specifier
            name: (type_identifier) @name
        ) @class

        (enum_specifier
            name: (type_identifier) @name
        ) @enum

        (type_definition
            declarator: (type_identifier) @name
        ) @type_alias

        (template_declaration
            (class_specifier
                name: (type_identifier) @name
            )
        ) @template_class

        (template_declaration
            (struct_specifier
                name: (type_identifier) @name
            )
        ) @template_struct
        "#
    }

    fn imports_query(&self) -> &str {
        r#"
        (preproc_include
            path: (string_literal) @import_path
        ) @include

        (preproc_include
            path: (system_lib_string) @import_path
        ) @system_include

        (using_declaration
            (qualified_identifier) @import_path
        ) @using
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Function calls
        (call_expression
            function: (identifier) @call_name
        ) @call

        (call_expression
            function: (field_expression
                field: (field_identifier) @method_call_name
            )
        ) @method_call

        (call_expression
            function: (qualified_identifier
                name: (identifier) @scoped_call_name
            )
        ) @scoped_call

        ; Type usages
        (type_identifier) @type_use

        ; Field access
        (field_expression
            field: (field_identifier) @field_access
        ) @field_expr

        ; Inheritance
        (base_class_clause
            (type_identifier) @extends_type
        )
        "#
    }

    fn cached_functions_query(&self) -> Option<&'static Query> {
        CPP_FUNCTIONS_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.functions_query())
        }).ok()
    }

    fn cached_types_query(&self) -> Option<&'static Query> {
        CPP_TYPES_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.types_query())
        }).ok()
    }

    fn cached_imports_query(&self) -> Option<&'static Query> {
        CPP_IMPORTS_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.imports_query())
        }).ok()
    }

    fn cached_references_query(&self) -> Option<&'static Query> {
        CPP_REFERENCES_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.references_query())
        }).ok()
    }
}
