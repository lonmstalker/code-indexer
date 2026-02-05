//! Project Compass Module
//!
//! Provides macro-level navigation for codebases including:
//! - Project profile (languages, frameworks, build tools)
//! - Module hierarchy
//! - Entry point detection

pub mod entry_detector;
pub mod node_builder;
pub mod profile_builder;

pub use entry_detector::{EntryDetector, EntryPoint, EntryType};
pub use node_builder::{NodeBuilder, ProjectNode, NodeType};
pub use profile_builder::{ProfileBuilder, ProjectProfile, LanguageStats, FrameworkInfo};
