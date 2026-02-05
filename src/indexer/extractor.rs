use std::path::Path;

use tree_sitter::StreamingIterator;

use crate::error::{IndexerError, Result};
use crate::index::{FileImport, ImportType, Location, ReferenceKind, Symbol, SymbolKind, SymbolReference, Visibility};
use crate::indexer::parser::ParsedFile;

/// Result of extraction containing symbols, references, and imports
#[derive(Debug, Default)]
pub struct ExtractionResult {
    pub symbols: Vec<Symbol>,
    pub references: Vec<SymbolReference>,
    pub imports: Vec<FileImport>,
}

pub struct SymbolExtractor;

impl SymbolExtractor {
    pub fn new() -> Self {
        Self
    }

    pub fn extract(&self, parsed: &ParsedFile, file_path: &Path) -> Result<Vec<Symbol>> {
        let result = self.extract_all(parsed, file_path)?;
        Ok(result.symbols)
    }

    /// Extract symbols, references, and imports from a parsed file
    pub fn extract_all(&self, parsed: &ParsedFile, file_path: &Path) -> Result<ExtractionResult> {
        let mut result = ExtractionResult::default();
        let file_path_str = file_path.to_string_lossy().to_string();

        self.extract_functions(parsed, &file_path_str, &mut result.symbols)?;
        self.extract_types(parsed, &file_path_str, &mut result.symbols)?;
        self.extract_references(parsed, &file_path_str, &mut result.references)?;
        self.extract_imports(parsed, &file_path_str, &mut result.imports)?;

        Ok(result)
    }

    fn extract_functions(
        &self,
        parsed: &ParsedFile,
        file_path: &str,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        let query_str = parsed.grammar.functions_query();
        if query_str.trim().is_empty() {
            return Ok(());
        }

        let query = tree_sitter::Query::new(&parsed.grammar.language(), query_str)
            .map_err(|e| IndexerError::Parse(format!("Invalid functions query: {}", e)))?;

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, parsed.root_node(), parsed.source_bytes());

        while let Some(m) = matches.next() {
            let mut name: Option<&str> = None;
            let mut kind = SymbolKind::Function;
            let mut node: Option<tree_sitter::Node> = None;
            let mut params_node: Option<tree_sitter::Node> = None;
            let mut signature_parts: Vec<&str> = Vec::new();
            let mut return_type_text: Option<&str> = None;

            for capture in m.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = parsed.node_text(&capture.node);

                match capture_name {
                    "name" | "method_name" => {
                        name = Some(text);
                        if capture_name == "method_name" {
                            kind = SymbolKind::Method;
                        }
                    }
                    "function" | "method" | "constructor" | "named_arrow" => {
                        node = Some(capture.node);
                        if capture_name == "constructor" {
                            kind = SymbolKind::Method;
                        }
                    }
                    "params" | "method_params" => {
                        params_node = Some(capture.node);
                        signature_parts.push(text);
                    }
                    "return_type" | "method_return_type" => {
                        return_type_text = Some(text);
                        signature_parts.push(text);
                    }
                    _ => {}
                }
            }

            if let (Some(name), Some(node)) = (name, node) {
                // For Rust: skip function_item inside impl blocks (they are captured as methods)
                // This prevents duplicate symbols for impl methods
                if kind == SymbolKind::Function && parsed.language == "rust" {
                    if let Some(parent) = node.parent() {
                        if parent.kind() == "declaration_list" {
                            if let Some(grandparent) = parent.parent() {
                                if grandparent.kind() == "impl_item" {
                                    continue; // Skip - will be captured as method
                                }
                            }
                        }
                    }
                }

                let location = Location::new(
                    file_path,
                    node.start_position().row as u32 + 1,
                    node.start_position().column as u32,
                    node.end_position().row as u32 + 1,
                    node.end_position().column as u32,
                );

                let mut symbol = Symbol::new(name, kind.clone(), location, &parsed.language);

                let visibility = self.extract_visibility(parsed, &node);
                if let Some(v) = visibility {
                    symbol = symbol.with_visibility(v);
                }

                let doc = self.extract_doc_comment(parsed, &node);
                if let Some(d) = doc {
                    symbol = symbol.with_doc_comment(d);
                }

                if !signature_parts.is_empty() {
                    symbol =
                        symbol.with_signature(format!("{}{}", name, signature_parts.join(" -> ")));
                }

                // Extract structured parameters for supported languages
                if let Some(pnode) = params_node {
                    let structured_params = match parsed.language.as_str() {
                        "python" => self.extract_python_params(parsed, &pnode),
                        "typescript" => self.extract_ts_params(parsed, &pnode),
                        "rust" => self.extract_rust_params(parsed, &pnode),
                        "java" => self.extract_java_params(parsed, &pnode),
                        "go" => self.extract_go_params(parsed, &pnode),
                        "cpp" => self.extract_cpp_params(parsed, &pnode),
                        "csharp" => self.extract_csharp_params(parsed, &pnode),
                        "swift" => self.extract_swift_params(parsed, &pnode),
                        "kotlin" => self.extract_kotlin_params(parsed, &pnode),
                        _ => Vec::new(),
                    };
                    if !structured_params.is_empty() {
                        symbol.params = structured_params;
                    }
                } else if parsed.language == "swift" {
                    // Swift: parameters are direct children of function_declaration, not wrapped in parameter_list
                    let structured_params = self.extract_swift_params_from_func(parsed, &node);
                    if !structured_params.is_empty() {
                        symbol.params = structured_params;
                    }
                }

                // Extract return type
                if let Some(rt) = return_type_text {
                    symbol.return_type = Some(rt.to_string());
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    fn extract_types(
        &self,
        parsed: &ParsedFile,
        file_path: &str,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        let query_str = parsed.grammar.types_query();
        if query_str.trim().is_empty() {
            return Ok(());
        }

        let query = tree_sitter::Query::new(&parsed.grammar.language(), query_str)
            .map_err(|e| IndexerError::Parse(format!("Invalid types query: {}", e)))?;

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, parsed.root_node(), parsed.source_bytes());

        while let Some(m) = matches.next() {
            let mut name: Option<&str> = None;
            let mut kind = SymbolKind::Struct;
            let mut node: Option<tree_sitter::Node> = None;

            for capture in m.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = parsed.node_text(&capture.node);

                match capture_name {
                    "name" => {
                        name = Some(text);
                    }
                    "struct" => {
                        node = Some(capture.node);
                        kind = SymbolKind::Struct;
                    }
                    "class" | "record" => {
                        node = Some(capture.node);
                        kind = SymbolKind::Class;
                    }
                    "interface" => {
                        node = Some(capture.node);
                        kind = SymbolKind::Interface;
                    }
                    "trait" => {
                        node = Some(capture.node);
                        kind = SymbolKind::Trait;
                    }
                    "enum" => {
                        node = Some(capture.node);
                        kind = SymbolKind::Enum;
                    }
                    "type_alias" => {
                        node = Some(capture.node);
                        kind = SymbolKind::TypeAlias;
                    }
                    _ => {}
                }
            }

            if let (Some(name), Some(node)) = (name, node) {
                let location = Location::new(
                    file_path,
                    node.start_position().row as u32 + 1,
                    node.start_position().column as u32,
                    node.end_position().row as u32 + 1,
                    node.end_position().column as u32,
                );

                let mut symbol = Symbol::new(name, kind, location, &parsed.language);

                let visibility = self.extract_visibility(parsed, &node);
                if let Some(v) = visibility {
                    symbol = symbol.with_visibility(v);
                }

                let doc = self.extract_doc_comment(parsed, &node);
                if let Some(d) = doc {
                    symbol = symbol.with_doc_comment(d);
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    fn extract_visibility(
        &self,
        parsed: &ParsedFile,
        node: &tree_sitter::Node,
    ) -> Option<Visibility> {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            let kind = child.kind();
            let text = parsed.node_text(&child);

            match kind {
                "visibility_modifier" => {
                    return Visibility::from_str(text);
                }
                "modifiers" => {
                    for modifier in child.children(&mut child.walk()) {
                        let modifier_text = parsed.node_text(&modifier);
                        if let Some(v) = Visibility::from_str(modifier_text) {
                            return Some(v);
                        }
                    }
                }
                _ => {
                    if text == "public" || text == "private" || text == "protected" {
                        return Visibility::from_str(text);
                    }
                }
            }
        }

        None
    }

    fn extract_doc_comment(
        &self,
        parsed: &ParsedFile,
        node: &tree_sitter::Node,
    ) -> Option<String> {
        if let Some(prev) = node.prev_sibling() {
            let kind = prev.kind();
            if kind.contains("comment") || kind == "line_comment" || kind == "block_comment" {
                let text = parsed.node_text(&prev).trim();
                if text.starts_with("///") || text.starts_with("/**") || text.starts_with("//!") {
                    return Some(text.to_string());
                }
            }
        }

        None
    }

    fn extract_references(
        &self,
        parsed: &ParsedFile,
        file_path: &str,
        references: &mut Vec<SymbolReference>,
    ) -> Result<()> {
        let query_str = parsed.grammar.references_query();
        if query_str.trim().is_empty() {
            return Ok(());
        }

        let query = match tree_sitter::Query::new(&parsed.grammar.language(), query_str) {
            Ok(q) => q,
            Err(e) => {
                // Log error but don't fail - references are optional
                tracing::warn!("Invalid references query for {}: {}", parsed.language, e);
                return Ok(());
            }
        };

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, parsed.root_node(), parsed.source_bytes());

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = parsed.node_text(&capture.node);
                let node = capture.node;

                let reference_kind = match capture_name {
                    "call_name" | "method_call_name" | "scoped_call_name" | "macro_name" => {
                        Some(ReferenceKind::Call)
                    }
                    "constructor_call_name" => Some(ReferenceKind::Call),
                    "type_use" => Some(ReferenceKind::TypeUse),
                    "impl_trait" | "extends_type" | "implements_type" => {
                        Some(ReferenceKind::Extend)
                    }
                    "field_access" | "field_access_name" | "property_access" | "static_access_name" => {
                        Some(ReferenceKind::FieldAccess)
                    }
                    _ => None,
                };

                if let Some(kind) = reference_kind {
                    // Skip common built-in types and keywords
                    if !Self::is_builtin_type(text, &parsed.language) {
                        references.push(SymbolReference::new(
                            text,
                            file_path,
                            node.start_position().row as u32 + 1,
                            node.start_position().column as u32,
                            kind,
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    fn extract_imports(
        &self,
        parsed: &ParsedFile,
        file_path: &str,
        imports: &mut Vec<FileImport>,
    ) -> Result<()> {
        let query_str = parsed.grammar.imports_query();
        if query_str.trim().is_empty() {
            return Ok(());
        }

        let query = match tree_sitter::Query::new(&parsed.grammar.language(), query_str) {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!("Invalid imports query for {}: {}", parsed.language, e);
                return Ok(());
            }
        };

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, parsed.root_node(), parsed.source_bytes());

        while let Some(m) = matches.next() {
            let mut import_path: Option<&str> = None;
            let mut is_wildcard = false;

            for capture in m.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = parsed.node_text(&capture.node);

                match capture_name {
                    "import_path" | "source" | "crate_name" => {
                        import_path = Some(text);
                        if text.ends_with('*') || text.contains("::*") {
                            is_wildcard = true;
                        }
                    }
                    _ => {}
                }
            }

            if let Some(path) = import_path {
                let import_type = if is_wildcard {
                    ImportType::Wildcard
                } else if path.contains("::") || path.contains('.') || path.contains('/') {
                    ImportType::Module
                } else {
                    ImportType::Symbol
                };

                // Extract the final symbol name if it's a specific import
                let imported_symbol = if import_type == ImportType::Symbol {
                    Some(path.to_string())
                } else {
                    path.rsplit(|c| c == ':' || c == '.' || c == '/')
                        .next()
                        .filter(|s| !s.is_empty() && *s != "*")
                        .map(|s| s.to_string())
                };

                imports.push(FileImport {
                    file_path: file_path.to_string(),
                    imported_path: Some(path.trim_matches('"').to_string()),
                    imported_symbol,
                    import_type,
                });
            }
        }

        Ok(())
    }

    /// Extract structured parameters from a Python function/method node
    fn extract_python_params(
        &self,
        parsed: &ParsedFile,
        params_node: &tree_sitter::Node,
    ) -> Vec<crate::index::FunctionParam> {
        let mut params = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    // Simple untyped parameter
                    let name = parsed.node_text(&child);
                    if name != "self" && name != "cls" {
                        params.push(crate::index::FunctionParam::new(name));
                    } else {
                        params.push(crate::index::FunctionParam::new(name).is_self_param());
                    }
                }
                "typed_parameter" => {
                    // Parameter with type hint: name: type
                    // Structure: identifier, ":", type (which contains identifier)
                    let mut name = "";
                    let mut type_ann: Option<String> = None;

                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        match c.kind() {
                            "identifier" if name.is_empty() => {
                                name = parsed.node_text(&c);
                            }
                            "type" => {
                                type_ann = Some(parsed.node_text(&c).to_string());
                            }
                            _ => {}
                        }
                    }

                    let mut param = crate::index::FunctionParam::new(name);
                    if let Some(t) = type_ann {
                        param = param.with_type(t);
                    }
                    if name == "self" || name == "cls" {
                        param = param.is_self_param();
                    }
                    params.push(param);
                }
                "default_parameter" => {
                    // Parameter with default: name = value
                    // Structure: identifier, "=", value
                    let mut name = "";
                    let mut default: Option<String> = None;

                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        match c.kind() {
                            "identifier" if name.is_empty() => {
                                name = parsed.node_text(&c);
                            }
                            _ if c.kind() != "=" && !name.is_empty() && default.is_none() => {
                                default = Some(parsed.node_text(&c).to_string());
                            }
                            _ => {}
                        }
                    }

                    let mut param = crate::index::FunctionParam::new(name);
                    if let Some(d) = default {
                        param = param.with_default(d);
                    }
                    params.push(param);
                }
                "typed_default_parameter" => {
                    // Parameter with type and default: name: type = value
                    // Structure: identifier, ":", type, "=", value
                    let mut name = "";
                    let mut type_ann: Option<String> = None;
                    let mut default: Option<String> = None;
                    let mut saw_equals = false;

                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        match c.kind() {
                            "identifier" if name.is_empty() => {
                                name = parsed.node_text(&c);
                            }
                            "type" => {
                                type_ann = Some(parsed.node_text(&c).to_string());
                            }
                            "=" => {
                                saw_equals = true;
                            }
                            _ if saw_equals && default.is_none() => {
                                default = Some(parsed.node_text(&c).to_string());
                            }
                            _ => {}
                        }
                    }

                    let mut param = crate::index::FunctionParam::new(name);
                    if let Some(t) = type_ann {
                        param = param.with_type(t);
                    }
                    if let Some(d) = default {
                        param = param.with_default(d);
                    }
                    params.push(param);
                }
                "list_splat_pattern" => {
                    // *args - structure contains identifier
                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        if c.kind() == "identifier" {
                            params.push(
                                crate::index::FunctionParam::new(parsed.node_text(&c))
                                    .variadic()
                            );
                            break;
                        }
                    }
                }
                "dictionary_splat_pattern" => {
                    // **kwargs - structure contains identifier
                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        if c.kind() == "identifier" {
                            params.push(
                                crate::index::FunctionParam::new(parsed.node_text(&c))
                                    .variadic()
                            );
                            break;
                        }
                    }
                }
                _ => {}
            }
        }

        params
    }

    /// Extract structured parameters from a TypeScript function/method node
    fn extract_ts_params(
        &self,
        parsed: &ParsedFile,
        params_node: &tree_sitter::Node,
    ) -> Vec<crate::index::FunctionParam> {
        let mut params = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "required_parameter" | "optional_parameter" => {
                    // Structure: identifier, type_annotation (contains ":" and type)
                    let mut name = "";
                    let mut type_ann: Option<String> = None;

                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        match c.kind() {
                            "identifier" if name.is_empty() => {
                                name = parsed.node_text(&c);
                            }
                            "type_annotation" => {
                                // type_annotation contains ":" and the actual type
                                // Extract just the type part (skip the colon)
                                let mut type_cursor = c.walk();
                                for tc in c.children(&mut type_cursor) {
                                    if tc.kind() != ":" {
                                        type_ann = Some(parsed.node_text(&tc).to_string());
                                        break;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    let mut param = crate::index::FunctionParam::new(name);
                    if let Some(t) = type_ann {
                        param = param.with_type(t);
                    }
                    if name == "this" {
                        param = param.is_self_param();
                    }
                    params.push(param);
                }
                "identifier" => {
                    // Simple parameter without type
                    let name = parsed.node_text(&child);
                    if name != "this" {
                        params.push(crate::index::FunctionParam::new(name));
                    } else {
                        params.push(crate::index::FunctionParam::new(name).is_self_param());
                    }
                }
                "rest_pattern" => {
                    // ...args - structure contains identifier
                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        if c.kind() == "identifier" {
                            params.push(
                                crate::index::FunctionParam::new(parsed.node_text(&c))
                                    .variadic()
                            );
                            break;
                        }
                    }
                }
                _ => {}
            }
        }

        params
    }

    /// Extract structured parameters from a Rust function/method node
    fn extract_rust_params(
        &self,
        parsed: &ParsedFile,
        params_node: &tree_sitter::Node,
    ) -> Vec<crate::index::FunctionParam> {
        let mut params = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "self_parameter" => {
                    // &self, &mut self, self
                    let text = parsed.node_text(&child);
                    let mut param = crate::index::FunctionParam::new("self").is_self_param();
                    // Extract the full type representation for &self or &mut self
                    if text.contains("&mut") {
                        param = param.with_type("&mut Self".to_string());
                    } else if text.contains('&') {
                        param = param.with_type("&Self".to_string());
                    } else {
                        param = param.with_type("Self".to_string());
                    }
                    params.push(param);
                }
                "parameter" => {
                    // Regular parameter: pattern: type
                    // Structure: pattern (identifier/reference_pattern/etc), ":", type
                    let mut name = "";
                    let mut type_ann: Option<String> = None;
                    let mut is_mutable = false;

                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        match c.kind() {
                            "identifier" if name.is_empty() => {
                                name = parsed.node_text(&c);
                            }
                            "mutable_specifier" => {
                                is_mutable = true;
                            }
                            "reference_pattern" => {
                                // &name or &mut name - extract the inner identifier
                                let mut ref_cursor = c.walk();
                                for rc in c.children(&mut ref_cursor) {
                                    if rc.kind() == "identifier" {
                                        name = parsed.node_text(&rc);
                                        break;
                                    }
                                }
                            }
                            _ => {
                                // Check if this is a type node (comes after ":")
                                if !name.is_empty() && c.kind() != ":" {
                                    type_ann = Some(parsed.node_text(&c).to_string());
                                }
                            }
                        }
                    }

                    if !name.is_empty() {
                        let mut param = crate::index::FunctionParam::new(name);
                        if let Some(t) = type_ann {
                            param = param.with_type(t);
                        }
                        if is_mutable {
                            // Store mutable info in the type if needed
                        }
                        params.push(param);
                    }
                }
                _ => {}
            }
        }

        params
    }

    /// Extract structured parameters from a Java method/constructor node
    fn extract_java_params(
        &self,
        parsed: &ParsedFile,
        params_node: &tree_sitter::Node,
    ) -> Vec<crate::index::FunctionParam> {
        let mut params = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "formal_parameter" | "spread_parameter" => {
                    // formal_parameter: [modifiers] type declarator_id
                    // spread_parameter: [modifiers] type ... variable_declarator
                    let mut name = "";
                    let mut type_ann: Option<String> = None;
                    let is_varargs = child.kind() == "spread_parameter";

                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        match c.kind() {
                            "identifier" => {
                                // This is the parameter name (last identifier)
                                name = parsed.node_text(&c);
                            }
                            "variable_declarator" => {
                                // In spread_parameter, the name may be in variable_declarator
                                // Extract the identifier from it
                                let mut vd_cursor = c.walk();
                                for vc in c.children(&mut vd_cursor) {
                                    if vc.kind() == "identifier" {
                                        name = parsed.node_text(&vc);
                                        break;
                                    }
                                }
                            }
                            "type_identifier" | "integral_type" | "floating_point_type"
                            | "boolean_type" | "void_type" | "array_type" | "generic_type" => {
                                type_ann = Some(parsed.node_text(&c).to_string());
                            }
                            "scoped_type_identifier" => {
                                // Qualified type like java.util.List
                                type_ann = Some(parsed.node_text(&c).to_string());
                            }
                            _ => {}
                        }
                    }

                    if !name.is_empty() {
                        let mut param = crate::index::FunctionParam::new(name);
                        if let Some(t) = type_ann {
                            param = param.with_type(t);
                        }
                        if is_varargs {
                            param = param.variadic();
                        }
                        params.push(param);
                    }
                }
                _ => {}
            }
        }

        params
    }

    /// Extract structured parameters from a Go function/method node
    fn extract_go_params(
        &self,
        parsed: &ParsedFile,
        params_node: &tree_sitter::Node,
    ) -> Vec<crate::index::FunctionParam> {
        let mut params = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "parameter_declaration" => {
                    // Go allows multiple names with one type: a, b int
                    // Structure: identifier(s), type
                    let mut names: Vec<&str> = Vec::new();
                    let mut type_ann: Option<String> = None;

                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        match c.kind() {
                            "identifier" => {
                                names.push(parsed.node_text(&c));
                            }
                            // Type nodes come after the names
                            "type_identifier" | "pointer_type" | "slice_type" | "array_type"
                            | "map_type" | "channel_type" | "function_type" | "interface_type"
                            | "struct_type" | "qualified_type" => {
                                type_ann = Some(parsed.node_text(&c).to_string());
                            }
                            _ => {}
                        }
                    }

                    // Create a param for each name with the same type
                    for name in names {
                        let mut param = crate::index::FunctionParam::new(name);
                        if let Some(ref t) = type_ann {
                            param = param.with_type(t.clone());
                        }
                        params.push(param);
                    }
                }
                "variadic_parameter_declaration" => {
                    // ...args type or identifier ...type
                    let mut name = "";
                    let mut type_ann: Option<String> = None;

                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        match c.kind() {
                            "identifier" => {
                                name = parsed.node_text(&c);
                            }
                            "type_identifier" | "pointer_type" | "slice_type" | "array_type"
                            | "map_type" | "qualified_type" => {
                                type_ann = Some(parsed.node_text(&c).to_string());
                            }
                            _ => {}
                        }
                    }

                    if !name.is_empty() {
                        let mut param = crate::index::FunctionParam::new(name);
                        if let Some(t) = type_ann {
                            param = param.with_type(format!("...{}", t));
                        }
                        param = param.variadic();
                        params.push(param);
                    }
                }
                _ => {}
            }
        }

        params
    }

    /// Extract structured parameters from a C++ function/method node
    fn extract_cpp_params(
        &self,
        parsed: &ParsedFile,
        params_node: &tree_sitter::Node,
    ) -> Vec<crate::index::FunctionParam> {
        let mut params = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "parameter_declaration" | "optional_parameter_declaration" => {
                    // C++ parameter: [type] [*&] declarator
                    // Examples: int x, const std::string& name, int* ptr
                    let mut name = "";
                    let mut type_parts: Vec<&str> = Vec::new();

                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        match c.kind() {
                            "identifier" => {
                                // Could be type or name, last identifier is usually the name
                                if !name.is_empty() {
                                    // Previous identifier was type
                                    type_parts.push(name);
                                }
                                name = parsed.node_text(&c);
                            }
                            "pointer_declarator" | "reference_declarator" => {
                                // Get the actual identifier from inside
                                if let Some(inner) = c.child_by_field_name("declarator") {
                                    name = parsed.node_text(&inner);
                                } else {
                                    // Try to find identifier child
                                    let mut inner_cursor = c.walk();
                                    for inner_child in c.children(&mut inner_cursor) {
                                        if inner_child.kind() == "identifier" {
                                            name = parsed.node_text(&inner_child);
                                            break;
                                        }
                                    }
                                }
                            }
                            "type_identifier" | "primitive_type" | "sized_type_specifier" => {
                                type_parts.push(parsed.node_text(&c));
                            }
                            "template_type" | "qualified_identifier" => {
                                type_parts.push(parsed.node_text(&c));
                            }
                            "type_qualifier" => {
                                // const, volatile, etc.
                                type_parts.push(parsed.node_text(&c));
                            }
                            "*" | "&" | "&&" => {
                                // Pointer or reference modifiers
                                type_parts.push(parsed.node_text(&c));
                            }
                            _ => {}
                        }
                    }

                    if !name.is_empty() {
                        let mut param = crate::index::FunctionParam::new(name);
                        if !type_parts.is_empty() {
                            param = param.with_type(type_parts.join(" "));
                        }
                        params.push(param);
                    }
                }
                "variadic_parameter" => {
                    // C++ variadic: ... (usually just ellipsis)
                    params.push(crate::index::FunctionParam::new("...").variadic());
                }
                _ => {}
            }
        }

        params
    }

    /// Extract structured parameters from a C# method/constructor node
    fn extract_csharp_params(
        &self,
        parsed: &ParsedFile,
        params_node: &tree_sitter::Node,
    ) -> Vec<crate::index::FunctionParam> {
        let mut params = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            if child.kind() == "parameter" {
                // C# parameter: [modifiers] type name [= default]
                // Examples: int x, string? name, ref int value, params string[] args
                let mut name = "";
                let mut type_ann: Option<String> = None;
                let mut is_params = false;

                let mut child_cursor = child.walk();
                for c in child.children(&mut child_cursor) {
                    match c.kind() {
                        "identifier" => {
                            // Last identifier is the parameter name
                            name = parsed.node_text(&c);
                        }
                        "predefined_type" | "nullable_type" | "array_type" | "generic_name"
                        | "qualified_name" => {
                            type_ann = Some(parsed.node_text(&c).to_string());
                        }
                        "parameter_modifier" => {
                            let mod_text = parsed.node_text(&c);
                            if mod_text == "params" {
                                is_params = true;
                            }
                        }
                        _ => {}
                    }
                }

                if !name.is_empty() {
                    let mut param = crate::index::FunctionParam::new(name);
                    if let Some(t) = type_ann {
                        param = param.with_type(t);
                    }
                    if is_params {
                        param = param.variadic();
                    }
                    params.push(param);
                }
            }
        }

        params
    }

    /// Extract structured parameters from a Swift function/method node
    fn extract_swift_params(
        &self,
        parsed: &ParsedFile,
        params_node: &tree_sitter::Node,
    ) -> Vec<crate::index::FunctionParam> {
        let mut params = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            if child.kind() == "parameter" {
                // Swift parameter: [external_name] local_name: Type [= default]
                // Examples: x: Int, named label: String, _ value: Bool
                let mut name = "";
                let mut type_ann: Option<String> = None;
                let mut is_variadic = false;

                let mut child_cursor = child.walk();
                for c in child.children(&mut child_cursor) {
                    match c.kind() {
                        "simple_identifier" => {
                            // First identifier might be external name, last is local name
                            name = parsed.node_text(&c);
                        }
                        "type_annotation" => {
                            // Get the actual type from inside type_annotation
                            let mut ta_cursor = c.walk();
                            for tc in c.children(&mut ta_cursor) {
                                match tc.kind() {
                                    "user_type" | "array_type" | "dictionary_type"
                                    | "optional_type" | "metatype" | "function_type"
                                    | "tuple_type" => {
                                        type_ann = Some(parsed.node_text(&tc).to_string());
                                    }
                                    _ => {}
                                }
                            }
                            // If we couldn't find a specific type, use the whole annotation minus ":"
                            if type_ann.is_none() {
                                let full = parsed.node_text(&c);
                                if full.starts_with(": ") {
                                    type_ann = Some(full[2..].to_string());
                                } else if full.starts_with(':') {
                                    type_ann = Some(full[1..].trim().to_string());
                                }
                            }
                        }
                        "..." => {
                            is_variadic = true;
                        }
                        _ => {}
                    }
                }

                if !name.is_empty() {
                    let mut param = crate::index::FunctionParam::new(name);
                    if let Some(t) = type_ann {
                        param = param.with_type(t);
                    }
                    if is_variadic {
                        param = param.variadic();
                    }
                    params.push(param);
                }
            }
        }

        params
    }

    /// Extract structured parameters from a Swift function_declaration node directly.
    /// Swift doesn't wrap parameters in a parameter_list - they are direct children of function_declaration.
    fn extract_swift_params_from_func(
        &self,
        parsed: &ParsedFile,
        func_node: &tree_sitter::Node,
    ) -> Vec<crate::index::FunctionParam> {
        let mut params = Vec::new();
        let mut cursor = func_node.walk();

        // Iterate over direct children of function_declaration looking for parameter nodes
        for child in func_node.children(&mut cursor) {
            if child.kind() == "parameter" {
                // Swift parameter structure:
                // parameter
                //   simple_identifier = "a"    <- parameter name
                //   :
                //   user_type
                //     type_identifier = "Int"  <- type
                let mut name = "";
                let mut type_ann: Option<String> = None;
                let mut is_variadic = false;

                let mut child_cursor = child.walk();
                for c in child.children(&mut child_cursor) {
                    match c.kind() {
                        "simple_identifier" => {
                            // The last simple_identifier is the local parameter name
                            name = parsed.node_text(&c);
                        }
                        "user_type" => {
                            // Get the type identifier
                            let mut type_cursor = c.walk();
                            for tc in c.children(&mut type_cursor) {
                                if tc.kind() == "type_identifier" {
                                    type_ann = Some(parsed.node_text(&tc).to_string());
                                    break;
                                }
                            }
                            // Fallback: use full user_type text
                            if type_ann.is_none() {
                                type_ann = Some(parsed.node_text(&c).to_string());
                            }
                        }
                        "array_type" | "dictionary_type" | "optional_type"
                        | "function_type" | "tuple_type" => {
                            type_ann = Some(parsed.node_text(&c).to_string());
                        }
                        "..." => {
                            is_variadic = true;
                        }
                        _ => {}
                    }
                }

                if !name.is_empty() {
                    let mut param = crate::index::FunctionParam::new(name);
                    if let Some(t) = type_ann {
                        param = param.with_type(t);
                    }
                    if is_variadic {
                        param = param.variadic();
                    }
                    params.push(param);
                }
            }
        }

        params
    }

    /// Extract structured parameters from a Kotlin function/method node
    fn extract_kotlin_params(
        &self,
        parsed: &ParsedFile,
        params_node: &tree_sitter::Node,
    ) -> Vec<crate::index::FunctionParam> {
        let mut params = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "parameter" | "function_value_parameter" => {
                    // Kotlin parameter: [vararg] name: Type [= default]
                    // Examples: x: Int, name: String = "", vararg items: Any
                    let mut name = "";
                    let mut type_ann: Option<String> = None;
                    let mut is_vararg = false;

                    let mut child_cursor = child.walk();
                    for c in child.children(&mut child_cursor) {
                        match c.kind() {
                            "simple_identifier" | "identifier" => {
                                name = parsed.node_text(&c);
                            }
                            "user_type" | "nullable_type" | "function_type" => {
                                type_ann = Some(parsed.node_text(&c).to_string());
                            }
                            "parameter_modifiers" | "parameter_modifier" => {
                                let mod_text = parsed.node_text(&c);
                                if mod_text.contains("vararg") {
                                    is_vararg = true;
                                }
                            }
                            "vararg" => {
                                is_vararg = true;
                            }
                            _ => {}
                        }
                    }

                    if !name.is_empty() {
                        let mut param = crate::index::FunctionParam::new(name);
                        if let Some(t) = type_ann {
                            param = param.with_type(t);
                        }
                        if is_vararg {
                            param = param.variadic();
                        }
                        params.push(param);
                    }
                }
                _ => {}
            }
        }

        params
    }

    /// Extract return type from a function node
    #[allow(dead_code)]
    fn extract_return_type(
        &self,
        parsed: &ParsedFile,
        node: &tree_sitter::Node,
        language: &str,
    ) -> Option<String> {
        match language {
            "python" => {
                // Look for return_type child
                node.child_by_field_name("return_type")
                    .map(|n| parsed.node_text(&n).to_string())
            }
            "typescript" => {
                // Look for return_type or type_annotation
                node.child_by_field_name("return_type")
                    .map(|n| parsed.node_text(&n).to_string())
            }
            "rust" => {
                // Look for return_type
                node.child_by_field_name("return_type")
                    .map(|n| parsed.node_text(&n).to_string())
            }
            _ => None,
        }
    }

    /// Check if a type name is a built-in/primitive type
    fn is_builtin_type(name: &str, language: &str) -> bool {
        match language {
            "rust" => matches!(
                name,
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
                    | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
                    | "f32" | "f64" | "bool" | "char" | "str" | "String"
                    | "Self" | "self" | "Vec" | "Option" | "Result" | "Box"
                    | "Rc" | "Arc" | "Cell" | "RefCell" | "Ok" | "Err" | "Some" | "None"
            ),
            "java" | "kotlin" => matches!(
                name,
                "int" | "long" | "short" | "byte" | "float" | "double" | "boolean" | "char"
                    | "void" | "Int" | "Long" | "Short" | "Byte" | "Float" | "Double"
                    | "Boolean" | "Char" | "String" | "Object" | "Unit" | "Any" | "Nothing"
            ),
            "typescript" => matches!(
                name,
                "string" | "number" | "boolean" | "void" | "null" | "undefined"
                    | "any" | "unknown" | "never" | "object" | "symbol" | "bigint"
                    | "String" | "Number" | "Boolean" | "Array" | "Object" | "Promise"
            ),
            _ => false,
        }
    }
}

impl Default for SymbolExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::parser::Parser;
    use crate::languages::LanguageRegistry;
    use std::path::PathBuf;

    fn parse_and_extract(source: &str, language: &str, filename: &str) -> Vec<Symbol> {
        let registry = LanguageRegistry::new();
        let parser = Parser::new(registry);
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name(language).unwrap();
        let parsed = parser.parse_source(source, grammar).unwrap();
        let extractor = SymbolExtractor::new();
        extractor.extract(&parsed, &PathBuf::from(filename)).unwrap()
    }

    // Rust tests
    #[test]
    fn test_extract_rust_function() {
        let source = r#"
fn hello_world() {
    println!("Hello");
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        assert!(!symbols.is_empty());
        let func = symbols.iter().find(|s| s.name == "hello_world");
        assert!(func.is_some());
        assert_eq!(func.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_rust_public_function() {
        let source = r#"
pub fn public_func() -> i32 {
    42
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let func = symbols.iter().find(|s| s.name == "public_func").unwrap();
        assert_eq!(func.visibility, Some(Visibility::Public));
    }

    #[test]
    fn test_extract_rust_struct() {
        let source = r#"
pub struct Point {
    x: f64,
    y: f64,
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let struct_sym = symbols.iter().find(|s| s.name == "Point");
        assert!(struct_sym.is_some());
        assert_eq!(struct_sym.unwrap().kind, SymbolKind::Struct);
    }

    #[test]
    fn test_extract_rust_enum() {
        let source = r#"
pub enum Color {
    Red,
    Green,
    Blue,
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let enum_sym = symbols.iter().find(|s| s.name == "Color");
        assert!(enum_sym.is_some());
        assert_eq!(enum_sym.unwrap().kind, SymbolKind::Enum);
    }

    #[test]
    fn test_extract_rust_trait() {
        let source = r#"
pub trait Drawable {
    fn draw(&self);
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let trait_sym = symbols.iter().find(|s| s.name == "Drawable");
        assert!(trait_sym.is_some());
        assert_eq!(trait_sym.unwrap().kind, SymbolKind::Trait);
    }

    #[test]
    fn test_extract_rust_impl_method() {
        let source = r#"
struct Calculator;

impl Calculator {
    pub fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let method = symbols.iter().find(|s| s.name == "add");
        assert!(method.is_some());
        assert_eq!(method.unwrap().kind, SymbolKind::Method);
    }

    #[test]
    fn test_extract_rust_type_alias() {
        let source = r#"
pub type Result<T> = std::result::Result<T, Error>;
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let alias = symbols.iter().find(|s| s.name == "Result");
        assert!(alias.is_some());
        assert_eq!(alias.unwrap().kind, SymbolKind::TypeAlias);
    }

    #[test]
    fn test_extract_rust_multiple_functions() {
        let source = r#"
fn func_a() {}
fn func_b() {}
fn func_c() {}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let funcs: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Function).collect();
        assert_eq!(funcs.len(), 3);
    }

    #[test]
    fn test_extract_rust_location() {
        let source = r#"fn test() {}"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let func = symbols.iter().find(|s| s.name == "test").unwrap();
        assert_eq!(func.location.file_path, "test.rs");
        assert!(func.location.start_line >= 1);
    }

    // Java tests
    #[test]
    fn test_extract_java_class() {
        let source = r#"
public class Calculator {
}
"#;
        let symbols = parse_and_extract(source, "java", "Calculator.java");

        let class = symbols.iter().find(|s| s.name == "Calculator");
        assert!(class.is_some());
        assert_eq!(class.unwrap().kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_java_method() {
        let source = r#"
public class Math {
    public int add(int a, int b) {
        return a + b;
    }
}
"#;
        let symbols = parse_and_extract(source, "java", "Math.java");

        let method = symbols.iter().find(|s| s.name == "add");
        assert!(method.is_some());
        // Java methods are extracted via @method capture which maps to Method kind
        let kind = method.unwrap().kind.clone();
        assert!(kind == SymbolKind::Method || kind == SymbolKind::Function);
    }

    #[test]
    fn test_extract_java_interface() {
        let source = r#"
public interface Printable {
    void print();
}
"#;
        let symbols = parse_and_extract(source, "java", "Printable.java");

        let iface = symbols.iter().find(|s| s.name == "Printable");
        assert!(iface.is_some());
        assert_eq!(iface.unwrap().kind, SymbolKind::Interface);
    }

    #[test]
    fn test_extract_java_enum() {
        let source = r#"
public enum Status {
    ACTIVE,
    INACTIVE
}
"#;
        let symbols = parse_and_extract(source, "java", "Status.java");

        let enum_sym = symbols.iter().find(|s| s.name == "Status");
        assert!(enum_sym.is_some());
        assert_eq!(enum_sym.unwrap().kind, SymbolKind::Enum);
    }

    #[test]
    fn test_extract_java_record() {
        let source = r#"
public record Point(int x, int y) {}
"#;
        let symbols = parse_and_extract(source, "java", "Point.java");

        let record = symbols.iter().find(|s| s.name == "Point");
        assert!(record.is_some());
        assert_eq!(record.unwrap().kind, SymbolKind::Class);
    }

    // TypeScript tests
    #[test]
    fn test_extract_typescript_function() {
        let source = r#"
function greet(name: string): string {
    return `Hello, ${name}!`;
}
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        let func = symbols.iter().find(|s| s.name == "greet");
        assert!(func.is_some());
        assert_eq!(func.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_typescript_interface() {
        let source = r#"
interface User {
    id: number;
    name: string;
}
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        let iface = symbols.iter().find(|s| s.name == "User");
        assert!(iface.is_some());
        assert_eq!(iface.unwrap().kind, SymbolKind::Interface);
    }

    #[test]
    fn test_extract_typescript_class() {
        let source = r#"
class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }
}
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        let class = symbols.iter().find(|s| s.name == "Calculator");
        assert!(class.is_some());
        assert_eq!(class.unwrap().kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_typescript_type_alias() {
        let source = r#"
type StringOrNumber = string | number;
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        let alias = symbols.iter().find(|s| s.name == "StringOrNumber");
        assert!(alias.is_some());
        assert_eq!(alias.unwrap().kind, SymbolKind::TypeAlias);
    }

    #[test]
    fn test_extract_typescript_enum() {
        let source = r#"
enum Direction {
    Up,
    Down,
    Left,
    Right
}
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        let enum_sym = symbols.iter().find(|s| s.name == "Direction");
        assert!(enum_sym.is_some());
        assert_eq!(enum_sym.unwrap().kind, SymbolKind::Enum);
    }

    #[test]
    fn test_extract_typescript_arrow_function() {
        let source = r#"
const add = (a: number, b: number): number => a + b;
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        // Arrow functions should be extracted as functions
        let func = symbols.iter().find(|s| s.name == "add");
        assert!(func.is_some(), "Arrow function 'add' should be extracted");
        assert_eq!(func.unwrap().kind, SymbolKind::Function);
    }

    // General tests
    #[test]
    fn test_extract_empty_source() {
        let symbols = parse_and_extract("", "rust", "test.rs");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_language_field() {
        let symbols = parse_and_extract("fn test() {}", "rust", "test.rs");
        assert!(!symbols.is_empty());
        assert_eq!(symbols[0].language, "rust");
    }

    #[test]
    fn test_extractor_default() {
        let extractor = SymbolExtractor::default();
        let registry = LanguageRegistry::new();
        let parser = Parser::new(registry);
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();
        let parsed = parser.parse_source("fn test() {}", grammar).unwrap();
        let symbols = extractor.extract(&parsed, &PathBuf::from("test.rs")).unwrap();
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_extract_symbols_have_unique_ids() {
        let source = r#"
fn func1() {}
fn func2() {}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let ids: Vec<_> = symbols.iter().map(|s| &s.id).collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique_ids.len());
    }

    // === Python Type Hints Tests ===

    #[test]
    fn test_extract_python_function_with_type_hints() {
        let source = r#"
def greet(name: str, age: int) -> str:
    return f"Hello {name}, you are {age}"
"#;
        let symbols = parse_and_extract(source, "python", "test.py");

        let func = symbols.iter().find(|s| s.name == "greet").unwrap();
        assert_eq!(func.kind, SymbolKind::Function);

        // Check return type
        assert_eq!(func.return_type, Some("str".to_string()));

        // Check params have type annotations
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "name");
        assert_eq!(func.params[0].type_annotation, Some("str".to_string()));
        assert_eq!(func.params[1].name, "age");
        assert_eq!(func.params[1].type_annotation, Some("int".to_string()));
    }

    #[test]
    fn test_extract_python_method_with_self() {
        let source = r#"
class User:
    def get_name(self, format: str) -> str:
        return self.name
"#;
        let symbols = parse_and_extract(source, "python", "test.py");

        let method = symbols.iter().find(|s| s.name == "get_name").unwrap();
        assert_eq!(method.kind, SymbolKind::Method);

        // self should be first param
        assert!(!method.params.is_empty());
        assert_eq!(method.params[0].name, "self");
        assert!(method.params[0].is_self);
    }

    #[test]
    fn test_extract_python_function_with_default_params() {
        let source = r#"
def connect(host: str, port: int = 8080) -> bool:
    pass
"#;
        let symbols = parse_and_extract(source, "python", "test.py");

        let func = symbols.iter().find(|s| s.name == "connect").unwrap();
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[1].name, "port");
        assert_eq!(func.params[1].type_annotation, Some("int".to_string()));
        assert_eq!(func.params[1].default_value, Some("8080".to_string()));
    }

    #[test]
    fn test_extract_python_function_without_type_hints() {
        let source = r#"
def legacy_func(x, y):
    return x + y
"#;
        let symbols = parse_and_extract(source, "python", "test.py");

        let func = symbols.iter().find(|s| s.name == "legacy_func").unwrap();
        assert_eq!(func.params.len(), 2);
        assert!(func.params[0].type_annotation.is_none());
        assert!(func.return_type.is_none());
    }

    #[test]
    fn test_extract_python_variadic_params() {
        let source = r#"
def varargs(*args, **kwargs):
    pass
"#;
        let symbols = parse_and_extract(source, "python", "test.py");

        let func = symbols.iter().find(|s| s.name == "varargs").unwrap();
        // Check variadic params are extracted
        let variadic_params: Vec<_> = func.params.iter().filter(|p| p.is_variadic).collect();
        assert_eq!(variadic_params.len(), 2);
    }

    // === TypeScript Type Tests ===

    #[test]
    fn test_extract_typescript_function_with_types() {
        let source = r#"
function add(a: number, b: number): number {
    return a + b;
}
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        let func = symbols.iter().find(|s| s.name == "add").unwrap();
        assert_eq!(func.kind, SymbolKind::Function);
        // Return type from query capture includes ": number"
        assert!(func.return_type.as_ref().map_or(false, |t| t.contains("number")));

        // Check params have correctly extracted types (without colon prefix)
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "a");
        assert_eq!(func.params[0].type_annotation, Some("number".to_string()));
        assert_eq!(func.params[1].name, "b");
        assert_eq!(func.params[1].type_annotation, Some("number".to_string()));
    }

    // === Rust Type Tests ===

    #[test]
    fn test_extract_rust_function_with_types() {
        let source = r#"
fn calculate(a: i32, b: String) -> bool {
    true
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let func = symbols.iter().find(|s| s.name == "calculate").unwrap();
        assert_eq!(func.kind, SymbolKind::Function);
        assert!(func.return_type.as_ref().map_or(false, |t| t.contains("bool")));

        // Check params have type annotations
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "a");
        assert_eq!(func.params[0].type_annotation, Some("i32".to_string()));
        assert_eq!(func.params[1].name, "b");
        assert_eq!(func.params[1].type_annotation, Some("String".to_string()));
    }

    #[test]
    fn test_extract_rust_method_with_self() {
        let source = r#"
struct Foo;

impl Foo {
    fn method(&self, x: i32) -> i32 {
        x
    }
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let method = symbols.iter().find(|s| s.name == "method").unwrap();
        assert_eq!(method.kind, SymbolKind::Method);

        // &self should be first param
        assert!(!method.params.is_empty());
        assert_eq!(method.params[0].name, "self");
        assert!(method.params[0].is_self);
        assert!(method.params[0].type_annotation.as_ref().map_or(false, |t| t.contains("Self")));

        // Second param should have type
        assert_eq!(method.params[1].name, "x");
        assert_eq!(method.params[1].type_annotation, Some("i32".to_string()));
    }

    #[test]
    fn test_extract_rust_method_with_mut_self() {
        let source = r#"
struct Bar;

impl Bar {
    fn mutate(&mut self, value: String) {
    }
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let method = symbols.iter().find(|s| s.name == "mutate").unwrap();

        // &mut self should include mut in type
        assert!(method.params[0].is_self);
        assert!(method.params[0].type_annotation.as_ref().map_or(false, |t| t.contains("mut")));
    }

    #[test]
    fn test_extract_rust_function_with_reference_params() {
        let source = r#"
fn process(data: &str, buffer: &mut Vec<u8>) -> &str {
    data
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let func = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "data");
        assert!(func.params[0].type_annotation.as_ref().map_or(false, |t| t.contains("&str")));
        assert_eq!(func.params[1].name, "buffer");
        assert!(func.params[1].type_annotation.as_ref().map_or(false, |t| t.contains("Vec")));
    }

    #[test]
    fn test_extract_rust_function_with_generic_types() {
        let source = r#"
fn transform<T: Clone>(item: T, items: Vec<T>) -> T {
    item.clone()
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let func = symbols.iter().find(|s| s.name == "transform").unwrap();
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "item");
        assert_eq!(func.params[0].type_annotation, Some("T".to_string()));
        assert_eq!(func.params[1].name, "items");
        assert!(func.params[1].type_annotation.as_ref().map_or(false, |t| t.contains("Vec")));
    }

    // === Java Type Tests ===

    #[test]
    fn test_extract_java_method_with_types() {
        let source = r#"
public class Example {
    public String greet(String name, int age) {
        return "Hello";
    }
}
"#;
        let symbols = parse_and_extract(source, "java", "Example.java");

        let method = symbols.iter().find(|s| s.name == "greet").unwrap();
        // Java methods inside classes are captured with the query
        assert!(method.kind == SymbolKind::Function || method.kind == SymbolKind::Method);

        // Check params have type annotations
        assert_eq!(method.params.len(), 2);
        assert_eq!(method.params[0].name, "name");
        assert_eq!(method.params[0].type_annotation, Some("String".to_string()));
        assert_eq!(method.params[1].name, "age");
        assert_eq!(method.params[1].type_annotation, Some("int".to_string()));
    }

    #[test]
    fn test_extract_java_constructor_params() {
        let source = r#"
public class User {
    public User(String name, int age) {
    }
}
"#;
        let symbols = parse_and_extract(source, "java", "User.java");

        let constructor = symbols.iter().find(|s| s.name == "User" && s.kind == SymbolKind::Method).unwrap();
        assert_eq!(constructor.params.len(), 2);
        assert_eq!(constructor.params[0].name, "name");
        assert_eq!(constructor.params[0].type_annotation, Some("String".to_string()));
    }

    #[test]
    fn test_extract_java_method_with_generic_types() {
        let source = r#"
public class Container {
    public <T> T transform(T item, List<T> items) {
        return item;
    }
}
"#;
        let symbols = parse_and_extract(source, "java", "Container.java");

        let method = symbols.iter().find(|s| s.name == "transform").unwrap();
        assert_eq!(method.params.len(), 2);
        assert_eq!(method.params[0].name, "item");
        assert_eq!(method.params[0].type_annotation, Some("T".to_string()));
        assert_eq!(method.params[1].name, "items");
        assert!(method.params[1].type_annotation.as_ref().map_or(false, |t| t.contains("List")));
    }

    #[test]
    fn test_extract_java_varargs() {
        let source = r#"
public class Formatter {
    public String format(String template, Object... args) {
        return "";
    }
}
"#;
        let symbols = parse_and_extract(source, "java", "Formatter.java");

        let method = symbols.iter().find(|s| s.name == "format").unwrap();
        // Both params should be captured
        assert_eq!(method.params.len(), 2);
        // First param is regular
        assert_eq!(method.params[0].name, "template");
        assert_eq!(method.params[0].type_annotation, Some("String".to_string()));
        assert!(!method.params[0].is_variadic);
        // Second param is varargs
        assert_eq!(method.params[1].name, "args");
        assert_eq!(method.params[1].type_annotation, Some("Object".to_string()));
        assert!(method.params[1].is_variadic);
    }

    #[test]
    fn test_extract_java_array_types() {
        let source = r#"
public class DataProcessor {
    public byte[] process(byte[] input, int[] indices) {
        return input;
    }
}
"#;
        let symbols = parse_and_extract(source, "java", "DataProcessor.java");

        let method = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(method.params.len(), 2);
        assert!(method.params[0].type_annotation.as_ref().map_or(false, |t| t.contains("byte")));
        assert!(method.params[1].type_annotation.as_ref().map_or(false, |t| t.contains("int")));
    }

    // === Go Type Tests ===

    #[test]
    fn test_extract_go_function_with_types() {
        let source = r#"
package main

func greet(name string, age int) string {
    return ""
}
"#;
        let symbols = parse_and_extract(source, "go", "main.go");

        let func = symbols.iter().find(|s| s.name == "greet").unwrap();
        assert_eq!(func.kind, SymbolKind::Function);

        // Check params have type annotations
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "name");
        assert_eq!(func.params[0].type_annotation, Some("string".to_string()));
        assert_eq!(func.params[1].name, "age");
        assert_eq!(func.params[1].type_annotation, Some("int".to_string()));
    }

    #[test]
    fn test_extract_go_function_grouped_params() {
        let source = r#"
package main

func add(a, b, c int) int {
    return a + b + c
}
"#;
        let symbols = parse_and_extract(source, "go", "main.go");

        let func = symbols.iter().find(|s| s.name == "add").unwrap();
        // Go allows grouping params with same type: a, b, c int
        assert_eq!(func.params.len(), 3);
        assert_eq!(func.params[0].name, "a");
        assert_eq!(func.params[0].type_annotation, Some("int".to_string()));
        assert_eq!(func.params[1].name, "b");
        assert_eq!(func.params[1].type_annotation, Some("int".to_string()));
        assert_eq!(func.params[2].name, "c");
        assert_eq!(func.params[2].type_annotation, Some("int".to_string()));
    }

    #[test]
    fn test_extract_go_method_with_receiver() {
        let source = r#"
package main

type Calculator struct{}

func (c *Calculator) Add(a, b int) int {
    return a + b
}
"#;
        let symbols = parse_and_extract(source, "go", "main.go");

        let method = symbols.iter().find(|s| s.name == "Add").unwrap();
        assert_eq!(method.kind, SymbolKind::Method);
        // Method params (not including receiver)
        assert_eq!(method.params.len(), 2);
        assert_eq!(method.params[0].name, "a");
        assert_eq!(method.params[1].name, "b");
    }

    #[test]
    fn test_extract_go_variadic_function() {
        let source = r#"
package main

func printf(format string, args ...interface{}) {
}
"#;
        let symbols = parse_and_extract(source, "go", "main.go");

        let func = symbols.iter().find(|s| s.name == "printf").unwrap();
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "format");
        assert_eq!(func.params[0].type_annotation, Some("string".to_string()));
        assert!(!func.params[0].is_variadic);
        // Variadic param
        assert_eq!(func.params[1].name, "args");
        assert!(func.params[1].is_variadic);
    }

    #[test]
    fn test_extract_go_pointer_and_slice_types() {
        let source = r#"
package main

func process(data []byte, ptr *int) *string {
    return nil
}
"#;
        let symbols = parse_and_extract(source, "go", "main.go");

        let func = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "data");
        assert!(func.params[0].type_annotation.as_ref().map_or(false, |t| t.contains("[]byte")));
        assert_eq!(func.params[1].name, "ptr");
        assert!(func.params[1].type_annotation.as_ref().map_or(false, |t| t.contains("*int")));
    }

    // C++ type extraction tests
    #[test]
    fn test_extract_cpp_function_params() {
        let source = r#"
int add(int a, int b) {
    return a + b;
}
"#;
        let symbols = parse_and_extract(source, "cpp", "test.cpp");

        let func = symbols.iter().find(|s| s.name == "add").unwrap();
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "a");
        assert_eq!(func.params[1].name, "b");
    }

    #[test]
    fn test_extract_cpp_const_ref_params() {
        let source = r#"
void process(const std::string& name, int* ptr) {
}
"#;
        let symbols = parse_and_extract(source, "cpp", "test.cpp");

        let func = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "name");
        assert_eq!(func.params[1].name, "ptr");
    }

    // C# type extraction tests
    #[test]
    fn test_extract_csharp_method_params() {
        let source = r#"
public class Calculator {
    public int Add(int a, int b) {
        return a + b;
    }
}
"#;
        let symbols = parse_and_extract(source, "csharp", "Calculator.cs");

        let method = symbols.iter().find(|s| s.name == "Add").unwrap();
        assert_eq!(method.params.len(), 2);
        assert_eq!(method.params[0].name, "a");
        assert!(method.params[0].type_annotation.as_ref().map_or(false, |t| t.contains("int")));
        assert_eq!(method.params[1].name, "b");
    }

    #[test]
    fn test_extract_csharp_nullable_params() {
        let source = r#"
public class Service {
    public void Process(string? name, int count) {
    }
}
"#;
        let symbols = parse_and_extract(source, "csharp", "Service.cs");

        let method = symbols.iter().find(|s| s.name == "Process").unwrap();
        assert_eq!(method.params.len(), 2);
        assert_eq!(method.params[0].name, "name");
        assert_eq!(method.params[1].name, "count");
    }

    // Swift type extraction tests
    #[test]
    fn test_extract_swift_function_params() {
        let source = r#"
func add(a: Int, b: Int) -> Int {
    return a + b
}
"#;
        let symbols = parse_and_extract(source, "swift", "test.swift");

        let func = symbols.iter().find(|s| s.name == "add").unwrap();
        assert_eq!(func.kind, SymbolKind::Function);
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "a");
        assert_eq!(func.params[0].type_annotation, Some("Int".to_string()));
        assert_eq!(func.params[1].name, "b");
        assert_eq!(func.params[1].type_annotation, Some("Int".to_string()));
    }

    #[test]
    fn test_extract_swift_optional_params() {
        let source = r#"
func process(name: String, count: Int) {
}
"#;
        let symbols = parse_and_extract(source, "swift", "test.swift");

        let func = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(func.kind, SymbolKind::Function);
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "name");
        assert_eq!(func.params[1].name, "count");
    }

    // Kotlin type extraction tests
    #[test]
    fn test_extract_kotlin_function_params() {
        let source = r#"
fun add(a: Int, b: Int): Int {
    return a + b
}
"#;
        let symbols = parse_and_extract(source, "kotlin", "test.kt");

        let func = symbols.iter().find(|s| s.name == "add").unwrap();
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "a");
        assert!(func.params[0].type_annotation.as_ref().map_or(false, |t| t.contains("Int")));
        assert_eq!(func.params[1].name, "b");
    }

    #[test]
    fn test_extract_kotlin_nullable_params() {
        let source = r#"
fun process(name: String?, count: Int) {
}
"#;
        let symbols = parse_and_extract(source, "kotlin", "test.kt");

        let func = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "name");
        assert_eq!(func.params[1].name, "count");
    }

    #[test]
    fn test_extract_kotlin_vararg_params() {
        let source = r#"
fun printf(format: String, vararg args: Any) {
}
"#;
        let symbols = parse_and_extract(source, "kotlin", "test.kt");

        let func = symbols.iter().find(|s| s.name == "printf").unwrap();
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "format");
        assert_eq!(func.params[1].name, "args");
        // Note: vararg detection requires grammar query to capture modifiers
        // Future improvement: detect "vararg" in parameter_modifiers node
    }
}
