use serde::{Deserialize, Serialize};

/// Output format for search results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OutputFormat {
    /// Full JSON output with all fields
    #[default]
    Full,
    /// Compact JSON with abbreviated field names: {n, k, f, l, s}
    Compact,
    /// Minimal single-line format: "name:kind@file:line"
    Minimal,
}

impl OutputFormat {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "full" | "json" => Some(OutputFormat::Full),
            "compact" => Some(OutputFormat::Compact),
            "minimal" | "min" => Some(OutputFormat::Minimal),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            OutputFormat::Full => "full",
            OutputFormat::Compact => "compact",
            OutputFormat::Minimal => "minimal",
        }
    }
}

/// Compact representation of a symbol for reduced token usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactSymbol {
    /// Symbol name
    pub n: String,
    /// Kind (abbreviated)
    pub k: String,
    /// File path
    pub f: String,
    /// Line number
    pub l: u32,
    /// Score (optional, for search results)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s: Option<f64>,
}

impl CompactSymbol {
    pub fn from_symbol(symbol: &Symbol, score: Option<f64>) -> Self {
        Self {
            n: symbol.name.clone(),
            k: symbol.kind.short_str().to_string(),
            f: symbol.location.file_path.clone(),
            l: symbol.location.start_line,
            s: score,
        }
    }

    /// Format as minimal single-line string: "name:kind@file:line"
    pub fn to_minimal_string(&self) -> String {
        if let Some(score) = self.s {
            format!("{}:{}@{}:{} [{:.2}]", self.n, self.k, self.f, self.l, score)
        } else {
            format!("{}:{}@{}:{}", self.n, self.k, self.f, self.l)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Class,
    Interface,
    Trait,
    Enum,
    EnumVariant,
    Constant,
    Variable,
    Field,
    Module,
    Import,
    TypeAlias,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Struct => "struct",
            SymbolKind::Class => "class",
            SymbolKind::Interface => "interface",
            SymbolKind::Trait => "trait",
            SymbolKind::Enum => "enum",
            SymbolKind::EnumVariant => "enum_variant",
            SymbolKind::Constant => "constant",
            SymbolKind::Variable => "variable",
            SymbolKind::Field => "field",
            SymbolKind::Module => "module",
            SymbolKind::Import => "import",
            SymbolKind::TypeAlias => "type_alias",
        }
    }

    /// Short string representation for compact output (2-3 chars)
    pub fn short_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "fn",
            SymbolKind::Method => "met",
            SymbolKind::Struct => "str",
            SymbolKind::Class => "cls",
            SymbolKind::Interface => "int",
            SymbolKind::Trait => "trt",
            SymbolKind::Enum => "enm",
            SymbolKind::EnumVariant => "var",
            SymbolKind::Constant => "cst",
            SymbolKind::Variable => "val",
            SymbolKind::Field => "fld",
            SymbolKind::Module => "mod",
            SymbolKind::Import => "imp",
            SymbolKind::TypeAlias => "typ",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "function" => Some(SymbolKind::Function),
            "method" => Some(SymbolKind::Method),
            "struct" => Some(SymbolKind::Struct),
            "class" => Some(SymbolKind::Class),
            "interface" => Some(SymbolKind::Interface),
            "trait" => Some(SymbolKind::Trait),
            "enum" => Some(SymbolKind::Enum),
            "enum_variant" => Some(SymbolKind::EnumVariant),
            "constant" => Some(SymbolKind::Constant),
            "variable" => Some(SymbolKind::Variable),
            "field" => Some(SymbolKind::Field),
            "module" => Some(SymbolKind::Module),
            "import" => Some(SymbolKind::Import),
            "type_alias" => Some(SymbolKind::TypeAlias),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Internal,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::Protected => "protected",
            Visibility::Internal => "internal",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "public" | "pub" => Some(Visibility::Public),
            "private" => Some(Visibility::Private),
            "protected" => Some(Visibility::Protected),
            "internal" | "pub(crate)" => Some(Visibility::Internal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub file_path: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl Location {
    pub fn new(
        file_path: impl Into<String>,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: String,
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
    pub language: String,
    pub visibility: Option<Visibility>,
    pub signature: Option<String>,
    pub doc_comment: Option<String>,
    pub parent: Option<String>,
    /// Scope ID where this symbol is defined
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<i64>,
    /// Fully Qualified Domain Name (e.g., "crate::module::Type::method")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fqdn: Option<String>,
}

impl Symbol {
    pub fn new(
        name: impl Into<String>,
        kind: SymbolKind,
        location: Location,
        language: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            kind,
            location,
            language: language.into(),
            visibility: None,
            signature: None,
            doc_comment: None,
            parent: None,
            scope_id: None,
            fqdn: None,
        }
    }

    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    pub fn with_signature(mut self, signature: impl Into<String>) -> Self {
        self.signature = Some(signature.into());
        self
    }

    pub fn with_doc_comment(mut self, doc: impl Into<String>) -> Self {
        self.doc_comment = Some(doc.into());
        self
    }

    #[allow(dead_code)]
    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    #[allow(dead_code)]
    pub fn with_scope_id(mut self, scope_id: i64) -> Self {
        self.scope_id = Some(scope_id);
        self
    }

    #[allow(dead_code)]
    pub fn with_fqdn(mut self, fqdn: impl Into<String>) -> Self {
        self.fqdn = Some(fqdn.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct FileInfo {
    pub path: String,
    pub language: String,
    pub last_modified: u64,
    pub symbol_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub total_files: usize,
    pub total_symbols: usize,
    pub symbols_by_kind: Vec<(String, usize)>,
    pub symbols_by_language: Vec<(String, usize)>,
    pub files_by_language: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub symbol: Symbol,
    pub score: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub limit: Option<usize>,
    pub kind_filter: Option<Vec<SymbolKind>>,
    pub language_filter: Option<Vec<String>>,
    pub file_filter: Option<String>,
    /// Name pattern filter (glob: * and ? supported, e.g., "format*")
    pub name_filter: Option<String>,
    /// Output format (full, compact, minimal)
    pub output_format: Option<OutputFormat>,
    /// Enable fuzzy search for typo tolerance
    pub fuzzy: Option<bool>,
    /// Fuzzy search threshold (0.0-1.0, default 0.7)
    pub fuzzy_threshold: Option<f64>,
    /// Current file path for locality-aware ranking
    pub current_file: Option<String>,
    /// Use advanced ranking with metrics (visibility, pagerank, recency)
    pub use_advanced_ranking: Option<bool>,
}

/// Type of reference to a symbol
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceKind {
    /// Function/method call
    Call,
    /// Type usage (e.g., variable declaration, function parameter)
    TypeUse,
    /// Import statement
    Import,
    /// Inheritance/implementation (extends, implements, impl for)
    Extend,
    /// Field access
    FieldAccess,
    /// Generic type argument
    TypeArgument,
}

impl ReferenceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReferenceKind::Call => "call",
            ReferenceKind::TypeUse => "type_use",
            ReferenceKind::Import => "import",
            ReferenceKind::Extend => "extend",
            ReferenceKind::FieldAccess => "field_access",
            ReferenceKind::TypeArgument => "type_argument",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "call" => Some(ReferenceKind::Call),
            "type_use" => Some(ReferenceKind::TypeUse),
            "import" => Some(ReferenceKind::Import),
            "extend" => Some(ReferenceKind::Extend),
            "field_access" => Some(ReferenceKind::FieldAccess),
            "type_argument" => Some(ReferenceKind::TypeArgument),
            _ => None,
        }
    }
}

/// A reference to a symbol from another location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolReference {
    /// ID of the referenced symbol (may be None if symbol is external)
    pub symbol_id: Option<String>,
    /// Name of the referenced symbol
    pub symbol_name: String,
    /// File where the reference occurs
    pub file_path: String,
    /// Line number of the reference
    pub line: u32,
    /// Column number of the reference
    pub column: u32,
    /// Type of reference
    pub kind: ReferenceKind,
}

impl SymbolReference {
    pub fn new(
        symbol_name: impl Into<String>,
        file_path: impl Into<String>,
        line: u32,
        column: u32,
        kind: ReferenceKind,
    ) -> Self {
        Self {
            symbol_id: None,
            symbol_name: symbol_name.into(),
            file_path: file_path.into(),
            line,
            column,
            kind,
        }
    }

    pub fn with_symbol_id(mut self, id: impl Into<String>) -> Self {
        self.symbol_id = Some(id.into());
        self
    }
}

/// An import in a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileImport {
    /// File containing the import
    pub file_path: String,
    /// Imported path/module (e.g., "std::collections::HashMap")
    pub imported_path: Option<String>,
    /// Specific imported symbol name (e.g., "HashMap")
    pub imported_symbol: Option<String>,
    /// Type of import
    pub import_type: ImportType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportType {
    /// Import a whole module
    Module,
    /// Import a specific symbol
    Symbol,
    /// Wildcard import (e.g., use std::*;)
    Wildcard,
}

impl ImportType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportType::Module => "module",
            ImportType::Symbol => "symbol",
            ImportType::Wildcard => "wildcard",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "module" => Some(ImportType::Module),
            "symbol" => Some(ImportType::Symbol),
            "wildcard" => Some(ImportType::Wildcard),
            _ => None,
        }
    }
}

// === Scope Models ===

/// Kind of scope in the code
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeKind {
    /// File-level scope
    File,
    /// Module scope (Rust mod, Python module, etc.)
    Module,
    /// Class scope
    Class,
    /// Function/method scope
    Function,
    /// Block scope (if, for, while, etc.)
    Block,
    /// Closure/lambda scope
    Closure,
}

impl ScopeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScopeKind::File => "file",
            ScopeKind::Module => "module",
            ScopeKind::Class => "class",
            ScopeKind::Function => "function",
            ScopeKind::Block => "block",
            ScopeKind::Closure => "closure",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "file" => Some(ScopeKind::File),
            "module" => Some(ScopeKind::Module),
            "class" => Some(ScopeKind::Class),
            "function" => Some(ScopeKind::Function),
            "block" => Some(ScopeKind::Block),
            "closure" => Some(ScopeKind::Closure),
            _ => None,
        }
    }
}

/// A scope in the code representing a lexical context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scope {
    /// Unique identifier for this scope
    pub id: i64,
    /// File path where this scope is defined
    pub file_path: String,
    /// Parent scope ID (None for file-level scope)
    pub parent_id: Option<i64>,
    /// Kind of scope
    pub kind: ScopeKind,
    /// Optional name (for named scopes like functions, classes)
    pub name: Option<String>,
    /// Start byte offset in the file
    pub start_offset: u32,
    /// End byte offset in the file
    pub end_offset: u32,
    /// Start line number
    pub start_line: u32,
    /// End line number
    pub end_line: u32,
}

// === Call Confidence Models ===

/// Confidence level for call graph edges
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallConfidence {
    /// Call target is definitely known (direct call, known type)
    Certain,
    /// Call target is possible but not certain (virtual dispatch, dynamic)
    Possible,
}

impl CallConfidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            CallConfidence::Certain => "certain",
            CallConfidence::Possible => "possible",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "certain" => Some(CallConfidence::Certain),
            "possible" => Some(CallConfidence::Possible),
            _ => None,
        }
    }
}

/// Reason why a call has uncertain target
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UncertaintyReason {
    /// Virtual dispatch through interface/trait
    VirtualDispatch,
    /// Receiver type is unknown at static analysis time
    DynamicReceiver,
    /// Multiple candidate implementations
    MultipleCandidates,
    /// Higher-order function (callback, closure parameter)
    HigherOrderFunction,
    /// External library call without source
    ExternalLibrary,
}

impl UncertaintyReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            UncertaintyReason::VirtualDispatch => "virtual_dispatch",
            UncertaintyReason::DynamicReceiver => "dynamic_receiver",
            UncertaintyReason::MultipleCandidates => "multiple_candidates",
            UncertaintyReason::HigherOrderFunction => "higher_order_function",
            UncertaintyReason::ExternalLibrary => "external_library",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "virtual_dispatch" => Some(UncertaintyReason::VirtualDispatch),
            "dynamic_receiver" => Some(UncertaintyReason::DynamicReceiver),
            "multiple_candidates" => Some(UncertaintyReason::MultipleCandidates),
            "higher_order_function" => Some(UncertaintyReason::HigherOrderFunction),
            "external_library" => Some(UncertaintyReason::ExternalLibrary),
            _ => None,
        }
    }
}

// === Symbol Metrics Models ===

/// Metrics for a symbol used in ranking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolMetrics {
    /// Symbol ID
    pub symbol_id: String,
    /// PageRank score (importance based on call graph)
    pub pagerank: f64,
    /// Number of incoming references
    pub incoming_refs: u32,
    /// Number of outgoing references
    pub outgoing_refs: u32,
    /// Git recency score (how recently modified)
    pub git_recency: f64,
}

// === Call Graph Models ===

/// Represents a call graph starting from an entry point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraph {
    pub nodes: Vec<CallGraphNode>,
    pub edges: Vec<CallGraphEdge>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

impl Default for CallGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// A node in the call graph representing a function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraphNode {
    /// Unique identifier (usually symbol ID)
    pub id: String,
    /// Function name
    pub name: String,
    /// File path where the function is defined
    pub file_path: String,
    /// Line number of the function definition
    pub line: u32,
    /// Depth in the call graph (0 = entry point)
    pub depth: u32,
}

/// An edge in the call graph representing a call relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraphEdge {
    /// ID of the calling function
    pub from: String,
    /// ID of the called function (None if unresolved)
    pub to: Option<String>,
    /// Name of the callee (for display/debugging)
    pub callee_name: String,
    /// Line number of the call site
    pub call_site_line: u32,
    /// Column number of the call site
    #[serde(default)]
    pub call_site_column: u32,
    /// File containing the call site
    pub call_site_file: String,
    /// Confidence level for this call edge
    #[serde(default = "default_confidence")]
    pub confidence: CallConfidence,
    /// Reason for uncertainty (if confidence is Possible)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<UncertaintyReason>,
}

fn default_confidence() -> CallConfidence {
    CallConfidence::Certain
}

// === Function Metrics Models ===

/// Metrics for a single function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetrics {
    /// Function name
    pub name: String,
    /// File path
    pub file_path: String,
    /// Lines of code (end_line - start_line + 1)
    pub loc: u32,
    /// Number of parameters
    pub parameters: u32,
    /// Start line number
    pub start_line: u32,
    /// End line number
    pub end_line: u32,
    /// Language
    pub language: String,
}

impl FunctionMetrics {
    pub fn from_symbol(symbol: &Symbol, param_count: u32) -> Self {
        Self {
            name: symbol.name.clone(),
            file_path: symbol.location.file_path.clone(),
            loc: symbol.location.end_line.saturating_sub(symbol.location.start_line) + 1,
            parameters: param_count,
            start_line: symbol.location.start_line,
            end_line: symbol.location.end_line,
            language: symbol.language.clone(),
        }
    }
}

/// Dead code analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadCodeReport {
    /// Unused functions (no call references)
    pub unused_functions: Vec<Symbol>,
    /// Unused types (no type_use references)
    pub unused_types: Vec<Symbol>,
    /// Total count of dead code items
    pub total_count: usize,
}

impl DeadCodeReport {
    pub fn new(unused_functions: Vec<Symbol>, unused_types: Vec<Symbol>) -> Self {
        let total_count = unused_functions.len() + unused_types.len();
        Self {
            unused_functions,
            unused_types,
            total_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SymbolKind tests
    #[test]
    fn test_symbol_kind_as_str() {
        assert_eq!(SymbolKind::Function.as_str(), "function");
        assert_eq!(SymbolKind::Method.as_str(), "method");
        assert_eq!(SymbolKind::Struct.as_str(), "struct");
        assert_eq!(SymbolKind::Class.as_str(), "class");
        assert_eq!(SymbolKind::Interface.as_str(), "interface");
        assert_eq!(SymbolKind::Trait.as_str(), "trait");
        assert_eq!(SymbolKind::Enum.as_str(), "enum");
        assert_eq!(SymbolKind::EnumVariant.as_str(), "enum_variant");
        assert_eq!(SymbolKind::Constant.as_str(), "constant");
        assert_eq!(SymbolKind::Variable.as_str(), "variable");
        assert_eq!(SymbolKind::Field.as_str(), "field");
        assert_eq!(SymbolKind::Module.as_str(), "module");
        assert_eq!(SymbolKind::Import.as_str(), "import");
        assert_eq!(SymbolKind::TypeAlias.as_str(), "type_alias");
    }

    #[test]
    fn test_symbol_kind_from_str() {
        assert_eq!(SymbolKind::from_str("function"), Some(SymbolKind::Function));
        assert_eq!(SymbolKind::from_str("method"), Some(SymbolKind::Method));
        assert_eq!(SymbolKind::from_str("struct"), Some(SymbolKind::Struct));
        assert_eq!(SymbolKind::from_str("class"), Some(SymbolKind::Class));
        assert_eq!(SymbolKind::from_str("interface"), Some(SymbolKind::Interface));
        assert_eq!(SymbolKind::from_str("trait"), Some(SymbolKind::Trait));
        assert_eq!(SymbolKind::from_str("enum"), Some(SymbolKind::Enum));
        assert_eq!(SymbolKind::from_str("enum_variant"), Some(SymbolKind::EnumVariant));
        assert_eq!(SymbolKind::from_str("constant"), Some(SymbolKind::Constant));
        assert_eq!(SymbolKind::from_str("variable"), Some(SymbolKind::Variable));
        assert_eq!(SymbolKind::from_str("field"), Some(SymbolKind::Field));
        assert_eq!(SymbolKind::from_str("module"), Some(SymbolKind::Module));
        assert_eq!(SymbolKind::from_str("import"), Some(SymbolKind::Import));
        assert_eq!(SymbolKind::from_str("type_alias"), Some(SymbolKind::TypeAlias));
    }

    #[test]
    fn test_symbol_kind_from_str_unknown() {
        assert_eq!(SymbolKind::from_str("unknown"), None);
        assert_eq!(SymbolKind::from_str(""), None);
        assert_eq!(SymbolKind::from_str("FUNCTION"), None);
    }

    #[test]
    fn test_symbol_kind_roundtrip() {
        let kinds = [
            SymbolKind::Function,
            SymbolKind::Method,
            SymbolKind::Struct,
            SymbolKind::Class,
            SymbolKind::Interface,
            SymbolKind::Trait,
            SymbolKind::Enum,
            SymbolKind::EnumVariant,
            SymbolKind::Constant,
            SymbolKind::Variable,
            SymbolKind::Field,
            SymbolKind::Module,
            SymbolKind::Import,
            SymbolKind::TypeAlias,
        ];

        for kind in kinds {
            let s = kind.as_str();
            let parsed = SymbolKind::from_str(s).unwrap();
            assert_eq!(kind, parsed);
        }
    }

    // Visibility tests
    #[test]
    fn test_visibility_as_str() {
        assert_eq!(Visibility::Public.as_str(), "public");
        assert_eq!(Visibility::Private.as_str(), "private");
        assert_eq!(Visibility::Protected.as_str(), "protected");
        assert_eq!(Visibility::Internal.as_str(), "internal");
    }

    #[test]
    fn test_visibility_from_str() {
        assert_eq!(Visibility::from_str("public"), Some(Visibility::Public));
        assert_eq!(Visibility::from_str("pub"), Some(Visibility::Public));
        assert_eq!(Visibility::from_str("private"), Some(Visibility::Private));
        assert_eq!(Visibility::from_str("protected"), Some(Visibility::Protected));
        assert_eq!(Visibility::from_str("internal"), Some(Visibility::Internal));
        assert_eq!(Visibility::from_str("pub(crate)"), Some(Visibility::Internal));
    }

    #[test]
    fn test_visibility_from_str_unknown() {
        assert_eq!(Visibility::from_str("unknown"), None);
        assert_eq!(Visibility::from_str(""), None);
        assert_eq!(Visibility::from_str("PUBLIC"), None);
    }

    // Location tests
    #[test]
    fn test_location_new() {
        let loc = Location::new("test.rs", 10, 5, 20, 1);
        assert_eq!(loc.file_path, "test.rs");
        assert_eq!(loc.start_line, 10);
        assert_eq!(loc.start_column, 5);
        assert_eq!(loc.end_line, 20);
        assert_eq!(loc.end_column, 1);
    }

    #[test]
    fn test_location_with_string() {
        let loc = Location::new(String::from("path/to/file.rs"), 1, 0, 5, 10);
        assert_eq!(loc.file_path, "path/to/file.rs");
    }

    #[test]
    fn test_location_equality() {
        let loc1 = Location::new("test.rs", 1, 0, 5, 10);
        let loc2 = Location::new("test.rs", 1, 0, 5, 10);
        let loc3 = Location::new("other.rs", 1, 0, 5, 10);
        assert_eq!(loc1, loc2);
        assert_ne!(loc1, loc3);
    }

    // Symbol tests
    #[test]
    fn test_symbol_new() {
        let loc = Location::new("test.rs", 1, 0, 5, 10);
        let symbol = Symbol::new("my_function", SymbolKind::Function, loc.clone(), "rust");

        assert!(!symbol.id.is_empty());
        assert_eq!(symbol.name, "my_function");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.location, loc);
        assert_eq!(symbol.language, "rust");
        assert!(symbol.visibility.is_none());
        assert!(symbol.signature.is_none());
        assert!(symbol.doc_comment.is_none());
        assert!(symbol.parent.is_none());
    }

    #[test]
    fn test_symbol_unique_ids() {
        let loc = Location::new("test.rs", 1, 0, 5, 10);
        let symbol1 = Symbol::new("func1", SymbolKind::Function, loc.clone(), "rust");
        let symbol2 = Symbol::new("func2", SymbolKind::Function, loc, "rust");
        assert_ne!(symbol1.id, symbol2.id);
    }

    #[test]
    fn test_symbol_with_visibility() {
        let loc = Location::new("test.rs", 1, 0, 5, 10);
        let symbol = Symbol::new("func", SymbolKind::Function, loc, "rust")
            .with_visibility(Visibility::Public);

        assert_eq!(symbol.visibility, Some(Visibility::Public));
    }

    #[test]
    fn test_symbol_with_signature() {
        let loc = Location::new("test.rs", 1, 0, 5, 10);
        let symbol = Symbol::new("func", SymbolKind::Function, loc, "rust")
            .with_signature("fn func(x: i32) -> i32");

        assert_eq!(symbol.signature, Some("fn func(x: i32) -> i32".to_string()));
    }

    #[test]
    fn test_symbol_with_doc_comment() {
        let loc = Location::new("test.rs", 1, 0, 5, 10);
        let symbol = Symbol::new("func", SymbolKind::Function, loc, "rust")
            .with_doc_comment("/// This is a doc comment");

        assert_eq!(symbol.doc_comment, Some("/// This is a doc comment".to_string()));
    }

    #[test]
    fn test_symbol_with_parent() {
        let loc = Location::new("test.rs", 1, 0, 5, 10);
        let symbol = Symbol::new("method", SymbolKind::Method, loc, "rust")
            .with_parent("MyStruct");

        assert_eq!(symbol.parent, Some("MyStruct".to_string()));
    }

    #[test]
    fn test_symbol_builder_chain() {
        let loc = Location::new("test.rs", 1, 0, 5, 10);
        let symbol = Symbol::new("method", SymbolKind::Method, loc, "rust")
            .with_visibility(Visibility::Public)
            .with_signature("fn method(&self) -> bool")
            .with_doc_comment("/// Returns true")
            .with_parent("MyStruct");

        assert_eq!(symbol.visibility, Some(Visibility::Public));
        assert_eq!(symbol.signature, Some("fn method(&self) -> bool".to_string()));
        assert_eq!(symbol.doc_comment, Some("/// Returns true".to_string()));
        assert_eq!(symbol.parent, Some("MyStruct".to_string()));
    }

    // SearchOptions tests
    #[test]
    fn test_search_options_default() {
        let opts = SearchOptions::default();
        assert!(opts.limit.is_none());
        assert!(opts.kind_filter.is_none());
        assert!(opts.language_filter.is_none());
        assert!(opts.file_filter.is_none());
    }

    // Serialization tests
    #[test]
    fn test_symbol_kind_serialization() {
        let kind = SymbolKind::Function;
        let json = serde_json::to_string(&kind).unwrap();
        let parsed: SymbolKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, parsed);
    }

    #[test]
    fn test_visibility_serialization() {
        let vis = Visibility::Public;
        let json = serde_json::to_string(&vis).unwrap();
        let parsed: Visibility = serde_json::from_str(&json).unwrap();
        assert_eq!(vis, parsed);
    }

    #[test]
    fn test_location_serialization() {
        let loc = Location::new("test.rs", 1, 0, 5, 10);
        let json = serde_json::to_string(&loc).unwrap();
        let parsed: Location = serde_json::from_str(&json).unwrap();
        assert_eq!(loc, parsed);
    }

    #[test]
    fn test_symbol_serialization() {
        let loc = Location::new("test.rs", 1, 0, 5, 10);
        let symbol = Symbol::new("func", SymbolKind::Function, loc, "rust")
            .with_visibility(Visibility::Public);

        let json = serde_json::to_string(&symbol).unwrap();
        let parsed: Symbol = serde_json::from_str(&json).unwrap();

        assert_eq!(symbol.id, parsed.id);
        assert_eq!(symbol.name, parsed.name);
        assert_eq!(symbol.kind, parsed.kind);
        assert_eq!(symbol.location, parsed.location);
        assert_eq!(symbol.visibility, parsed.visibility);
    }
}
