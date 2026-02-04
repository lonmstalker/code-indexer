pub mod call_analyzer;
pub mod extractor;
pub mod import_resolver;
pub mod parser;
pub mod resolver;
pub mod scope_builder;
pub mod walker;
pub mod watcher;

pub use call_analyzer::{CallAnalysisResult, CallAnalyzer};
pub use extractor::{ExtractionResult, SymbolExtractor};
pub use import_resolver::{
    GoImportResolver, ImportResolver, ImportResolverRegistry, JavaImportResolver,
    PythonImportResolver, RustImportResolver, TypeScriptImportResolver,
};
pub use parser::Parser;
pub use resolver::{compute_fqdn, ScopeResolver};
pub use scope_builder::{scope_at_offset, scope_chain, ScopeBuilder};
pub use walker::FileWalker;
pub use watcher::FileWatcher;
