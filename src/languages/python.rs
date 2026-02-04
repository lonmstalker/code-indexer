use once_cell::sync::OnceCell;
use tree_sitter::Query;

use super::LanguageGrammar;

pub struct PythonGrammar;

// Static query caches for Python
static PYTHON_FUNCTIONS_QUERY: OnceCell<Query> = OnceCell::new();
static PYTHON_TYPES_QUERY: OnceCell<Query> = OnceCell::new();
static PYTHON_IMPORTS_QUERY: OnceCell<Query> = OnceCell::new();
static PYTHON_REFERENCES_QUERY: OnceCell<Query> = OnceCell::new();

impl LanguageGrammar for PythonGrammar {
    fn name(&self) -> &'static str {
        "python"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["py", "pyi"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        (function_definition
            name: (identifier) @name
            parameters: (parameters) @params
            return_type: (_)? @return_type
        ) @function

        (class_definition
            body: (block
                (function_definition
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
        (class_definition
            name: (identifier) @name
        ) @class
        "#
    }

    fn imports_query(&self) -> &str {
        r#"
        (import_statement
            name: (dotted_name) @import_path
        ) @import

        (import_from_statement
            module_name: (dotted_name) @module
            name: (dotted_name)? @import_path
        ) @from_import

        (aliased_import
            name: (dotted_name) @import_path
        )
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Function calls
        (call
            function: (identifier) @call_name
        ) @call

        (call
            function: (attribute
                attribute: (identifier) @method_call_name
            )
        ) @method_call

        ; Type annotations
        (type
            (identifier) @type_use
        )

        ; Attribute access
        (attribute
            attribute: (identifier) @field_access
        ) @field_expr

        ; Inheritance
        (class_definition
            superclasses: (argument_list
                (identifier) @extends_type
            )
        ) @class_inheritance
        "#
    }

    fn cached_functions_query(&self) -> Option<&'static Query> {
        PYTHON_FUNCTIONS_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.functions_query())
        }).ok()
    }

    fn cached_types_query(&self) -> Option<&'static Query> {
        PYTHON_TYPES_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.types_query())
        }).ok()
    }

    fn cached_imports_query(&self) -> Option<&'static Query> {
        PYTHON_IMPORTS_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.imports_query())
        }).ok()
    }

    fn cached_references_query(&self) -> Option<&'static Query> {
        PYTHON_REFERENCES_QUERY.get_or_try_init(|| {
            Query::new(&self.language(), self.references_query())
        }).ok()
    }
}
