pub mod dependencies;
pub mod error;
pub mod index;
pub mod indexer;
pub mod languages;
pub mod memory;
pub mod workspace;

use once_cell::sync::Lazy;

pub use dependencies::{
    CargoResolver, Dependency, DependencyRegistry, DependencyResolver, Ecosystem, NpmResolver,
    ProjectInfo, SymbolSource,
};
pub use error::{IndexerError, Result};
pub use index::sqlite::SqliteIndex;
pub use index::{
    CallGraph, CallGraphEdge, CallGraphNode, CodeIndex, DeadCodeReport, FileImport,
    FunctionMetrics, ImportType, ReferenceKind, SearchOptions, Symbol, SymbolKind,
    SymbolReference,
};
pub use indexer::{ExtractionResult, FileWalker, Parser, SymbolExtractor};
pub use languages::{CrossLanguageAnalyzer, CrossLanguageRef, CrossRefType, LanguageRegistry};
pub use memory::{ArchitectureAnalyzer, ArchitectureSummary, CodeConventions, ProjectContext};
pub use workspace::{ModuleInfo, ModuleType, WorkspaceDetector, WorkspaceInfo, WorkspaceType};

/// Global language registry instance (lazily initialized)
pub static REGISTRY: Lazy<LanguageRegistry> = Lazy::new(LanguageRegistry::new);
