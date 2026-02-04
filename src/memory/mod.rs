//! Memory Bank integration for project context.
//!
//! This module provides automatic analysis and extraction of project context
//! that can be used by AI agents to better understand the codebase.

pub mod analyzer;
pub mod context;

pub use analyzer::ArchitectureAnalyzer;
pub use context::{ProjectContext, ArchitectureSummary, CodeConventions};
