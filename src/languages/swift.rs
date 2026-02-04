use super::LanguageGrammar;

pub struct SwiftGrammar;

impl LanguageGrammar for SwiftGrammar {
    fn name(&self) -> &'static str {
        "swift"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["swift"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_swift::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        ; Function declarations
        (function_declaration
            name: (simple_identifier) @name
        ) @function

        ; Initializers
        (init_declaration) @initializer

        ; Deinitializers
        (deinit_declaration) @deinitializer

        ; Subscript declarations
        (subscript_declaration) @subscript
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        ; All type declarations (class, struct, enum, actor, extension)
        (class_declaration
            name: (type_identifier) @name
        ) @type

        (class_declaration
            name: (user_type
                (type_identifier) @name
            )
        ) @type

        ; Protocol declarations
        (protocol_declaration
            name: (type_identifier) @name
        ) @protocol

        (protocol_declaration
            name: (user_type
                (type_identifier) @name
            )
        ) @protocol

        ; Typealias declarations
        (typealias_declaration
            name: (type_identifier) @name
        ) @typealias
        "#
    }

    fn imports_query(&self) -> &str {
        r#"
        ; Import statements
        (import_declaration
            (identifier) @import_path
        ) @import
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Function calls
        (call_expression
            (simple_identifier) @call_name
        ) @call

        ; Method calls via navigation
        (call_expression
            (navigation_expression
                (simple_identifier) @method_call
            )
        ) @method_call

        ; Type references
        (type_identifier) @type_ref

        ; Property access
        (navigation_expression
            (simple_identifier) @property_access
        ) @access
        "#
    }
}
