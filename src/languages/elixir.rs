use super::LanguageGrammar;

pub struct ElixirGrammar;

impl LanguageGrammar for ElixirGrammar {
    fn name(&self) -> &'static str {
        "elixir"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["ex", "exs"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_elixir::LANGUAGE.into()
    }

    fn functions_query(&self) -> &str {
        r#"
        ; Function definitions (def)
        (call
            target: (identifier) @fn_type
            (#match? @fn_type "^(def|defp|defmacro|defmacrop|defguard|defguardp)$")
            (arguments
                (call
                    target: (identifier) @name
                )
            )
        ) @function

        ; Function definitions with simple identifier
        (call
            target: (identifier) @fn_type
            (#match? @fn_type "^(def|defp|defmacro|defmacrop|defguard|defguardp)$")
            (arguments
                (identifier) @name
            )
        ) @function
        "#
    }

    fn types_query(&self) -> &str {
        r#"
        ; Module definitions
        (call
            target: (identifier) @mod_type
            (#eq? @mod_type "defmodule")
            (arguments
                (alias) @name
            )
        ) @module

        ; Struct definitions
        (call
            target: (identifier) @struct_type
            (#eq? @struct_type "defstruct")
        ) @struct

        ; Protocol definitions
        (call
            target: (identifier) @proto_type
            (#eq? @proto_type "defprotocol")
            (arguments
                (alias) @name
            )
        ) @protocol

        ; Implementation definitions
        (call
            target: (identifier) @impl_type
            (#eq? @impl_type "defimpl")
            (arguments
                (alias) @name
            )
        ) @impl

        ; Exception definitions
        (call
            target: (identifier) @exc_type
            (#eq? @exc_type "defexception")
        ) @exception
        "#
    }

    fn imports_query(&self) -> &str {
        r#"
        ; Alias statements
        (call
            target: (identifier) @stmt_type
            (#eq? @stmt_type "alias")
            (arguments
                (alias) @import_path
            )
        ) @alias

        ; Import statements
        (call
            target: (identifier) @stmt_type
            (#eq? @stmt_type "import")
            (arguments
                (alias) @import_path
            )
        ) @import

        ; Require statements
        (call
            target: (identifier) @stmt_type
            (#eq? @stmt_type "require")
            (arguments
                (alias) @import_path
            )
        ) @require

        ; Use statements
        (call
            target: (identifier) @stmt_type
            (#eq? @stmt_type "use")
            (arguments
                (alias) @import_path
            )
        ) @use
        "#
    }

    fn references_query(&self) -> &str {
        r#"
        ; Function calls
        (call
            target: (identifier) @call_name
        ) @call

        ; Remote function calls (Module.function)
        (call
            target: (dot
                left: (alias) @module
                right: (identifier) @remote_call
            )
        ) @remote_call

        ; Module references
        (alias) @module_ref

        ; Variable references
        (identifier) @var_ref
        "#
    }
}
