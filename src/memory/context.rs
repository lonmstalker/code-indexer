//! Project context models for Memory Bank integration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete project context for AI agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    /// Project name
    pub project_name: String,
    /// Primary languages used
    pub languages: Vec<String>,
    /// Ecosystem/package managers used
    pub ecosystems: Vec<String>,
    /// Architecture summary
    pub architecture: ArchitectureSummary,
    /// Detected code conventions
    pub conventions: CodeConventions,
    /// Important files in the project
    pub important_files: Vec<String>,
    /// Project description (from README or manifest)
    pub description: Option<String>,
}

impl ProjectContext {
    pub fn new(project_name: impl Into<String>) -> Self {
        Self {
            project_name: project_name.into(),
            languages: Vec::new(),
            ecosystems: Vec::new(),
            architecture: ArchitectureSummary::default(),
            conventions: CodeConventions::default(),
            important_files: Vec::new(),
            description: None,
        }
    }
}

/// Summary of project architecture
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArchitectureSummary {
    /// Top-level modules/packages
    pub modules: Vec<ModuleSummary>,
    /// Entry points (main files, binaries)
    pub entry_points: Vec<String>,
    /// Key types (most important structs/classes)
    pub key_types: Vec<TypeSummary>,
    /// Key functions (most important functions)
    pub key_functions: Vec<FunctionSummary>,
    /// Detected architectural patterns
    pub patterns: Vec<String>,
}

/// Summary of a module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSummary {
    /// Module name
    pub name: String,
    /// Module path
    pub path: String,
    /// Number of symbols in module
    pub symbol_count: usize,
    /// Brief description of module purpose
    pub purpose: Option<String>,
}

/// Summary of a type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeSummary {
    /// Type name
    pub name: String,
    /// Type kind (struct, class, trait, interface, enum)
    pub kind: String,
    /// File where type is defined
    pub file: String,
    /// Number of methods
    pub method_count: usize,
    /// Is it a key/core type
    pub is_key: bool,
}

/// Summary of a function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSummary {
    /// Function name
    pub name: String,
    /// File where function is defined
    pub file: String,
    /// Function signature
    pub signature: Option<String>,
    /// Is it a public API
    pub is_public: bool,
}

/// Detected code conventions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeConventions {
    /// Error handling pattern
    pub error_handling: Option<String>,
    /// Async runtime (if any)
    pub async_runtime: Option<String>,
    /// Serialization library
    pub serialization: Option<String>,
    /// Testing framework
    pub testing_framework: Option<String>,
    /// Logging library
    pub logging: Option<String>,
    /// HTTP framework (if any)
    pub http_framework: Option<String>,
    /// Database library (if any)
    pub database: Option<String>,
    /// Naming conventions
    pub naming: NamingConventions,
    /// Other detected patterns
    pub other: HashMap<String, String>,
}

/// Naming conventions detected in the codebase
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NamingConventions {
    /// Function naming style (snake_case, camelCase, etc.)
    pub functions: Option<String>,
    /// Type naming style (PascalCase, etc.)
    pub types: Option<String>,
    /// Constant naming style (SCREAMING_SNAKE_CASE, etc.)
    pub constants: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_context_new() {
        let ctx = ProjectContext::new("my-project");
        assert_eq!(ctx.project_name, "my-project");
        assert!(ctx.languages.is_empty());
        assert!(ctx.ecosystems.is_empty());
    }

    #[test]
    fn test_project_context_serialization() {
        let ctx = ProjectContext::new("test");
        let json = serde_json::to_string(&ctx).unwrap();
        let parsed: ProjectContext = serde_json::from_str(&json).unwrap();
        assert_eq!(ctx.project_name, parsed.project_name);
    }

    #[test]
    fn test_architecture_summary_default() {
        let summary = ArchitectureSummary::default();
        assert!(summary.modules.is_empty());
        assert!(summary.entry_points.is_empty());
        assert!(summary.key_types.is_empty());
    }

    #[test]
    fn test_code_conventions_default() {
        let conventions = CodeConventions::default();
        assert!(conventions.error_handling.is_none());
        assert!(conventions.async_runtime.is_none());
    }
}
