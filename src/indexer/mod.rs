pub mod call_analyzer;
pub mod extractor;
pub mod import_resolver;
pub mod parser;
pub mod progress;
pub mod resolver;
pub mod scope_builder;
pub mod sidecar;
pub mod walker;
pub mod watcher;

pub use call_analyzer::{CallAnalysisResult, CallAnalyzer};
pub use extractor::{ExtractionResult, SymbolExtractor};
pub use import_resolver::{
    GoImportResolver, ImportResolver, ImportResolverRegistry, JavaImportResolver,
    PythonImportResolver, RustImportResolver, TypeScriptImportResolver,
};
pub use parser::{ParseCache, Parser};
pub use progress::IndexingProgress;
pub use resolver::{compute_fqdn, ScopeResolver};
pub use scope_builder::{scope_at_offset, scope_chain, ScopeBuilder};
pub use sidecar::{
    apply_tag_rules, check_staleness, compute_exported_hash, extract_file_meta, extract_file_tags,
    default_agent_api_key_env, extract_front_matter, find_sidecar_path, normalize_agent_provider,
    parse_sidecar, parse_tag, preview_tag_rules, resolve_agent_api_key, resolve_inferred_tags,
    resolve_tags, resolve_tags_with_warnings, FileMetadata, InferredTag, ResolvedTagsResult,
    RootSidecarData, SidecarData, TagRule, TagRuleMatch, SIDECAR_FILENAME,
};
pub use walker::FileWalker;
pub use watcher::FileWatcher;
