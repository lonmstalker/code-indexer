//! Architecture analysis for automatic context extraction.

use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;
use crate::index::{CodeIndex, SearchOptions, Symbol};
use crate::workspace::{WorkspaceDetector, WorkspaceType};

use super::context::{
    ArchitectureSummary, CodeConventions, FunctionSummary, ModuleSummary, NamingConventions,
    ProjectContext, TypeSummary,
};

/// Analyzer for extracting project architecture and conventions
pub struct ArchitectureAnalyzer;

impl ArchitectureAnalyzer {
    /// Analyze a project and extract its context
    pub fn analyze<I: CodeIndex>(project_path: &Path, index: &I) -> Result<ProjectContext> {
        let mut context = ProjectContext::new(
            project_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
        );

        // Detect workspace type and get project info
        let workspace_type = WorkspaceDetector::detect(project_path);
        Self::detect_ecosystems(&mut context, &workspace_type);

        // Analyze languages from index
        Self::analyze_languages(&mut context, index)?;

        // Analyze architecture
        context.architecture = Self::analyze_architecture(project_path, index)?;

        // Detect conventions
        context.conventions = Self::detect_conventions(project_path, index)?;

        // Find important files
        context.important_files = Self::find_important_files(project_path);

        // Try to get description from README or manifest
        context.description = Self::extract_description(project_path);

        Ok(context)
    }

    fn detect_ecosystems(context: &mut ProjectContext, workspace_type: &WorkspaceType) {
        match workspace_type {
            WorkspaceType::CargoWorkspace | WorkspaceType::SingleProject => {
                let cargo_toml = Path::new(".").join("Cargo.toml");
                if cargo_toml.exists() {
                    context.ecosystems.push("cargo".to_string());
                }
            }
            WorkspaceType::NpmWorkspace => {
                context.ecosystems.push("npm".to_string());
            }
            WorkspaceType::GradleMultiProject => {
                context.ecosystems.push("gradle".to_string());
            }
            WorkspaceType::MavenMultiModule => {
                context.ecosystems.push("maven".to_string());
            }
        }
    }

    fn analyze_languages<I: CodeIndex>(context: &mut ProjectContext, index: &I) -> Result<()> {
        let stats = index.get_stats()?;

        // Sort languages by file count
        let mut lang_files: Vec<_> = stats.files_by_language.clone();
        lang_files.sort_by(|a, b| b.1.cmp(&a.1));

        context.languages = lang_files.into_iter().map(|(lang, _)| lang).collect();

        Ok(())
    }

    fn analyze_architecture<I: CodeIndex>(
        project_path: &Path,
        index: &I,
    ) -> Result<ArchitectureSummary> {
        let mut summary = ArchitectureSummary::default();

        // Detect entry points
        summary.entry_points = Self::find_entry_points(project_path);

        // Get key types
        let types = index.list_types(&SearchOptions::default())?;
        summary.key_types = Self::identify_key_types(&types);

        // Get key functions
        let functions = index.list_functions(&SearchOptions::default())?;
        summary.key_functions = Self::identify_key_functions(&functions);

        // Identify modules
        summary.modules = Self::identify_modules(&types, &functions);

        // Detect patterns
        summary.patterns = Self::detect_patterns(&types, &functions);

        Ok(summary)
    }

    fn find_entry_points(project_path: &Path) -> Vec<String> {
        let mut entry_points = Vec::new();

        // Rust entry points
        for path in ["src/main.rs", "src/lib.rs", "src/bin"] {
            let full_path = project_path.join(path);
            if full_path.exists() {
                entry_points.push(path.to_string());
            }
        }

        // Java entry points
        for pattern in ["src/main/java", "src/main/kotlin", "app/src/main"] {
            let full_path = project_path.join(pattern);
            if full_path.exists() {
                entry_points.push(pattern.to_string());
            }
        }

        // TypeScript entry points
        for path in ["src/index.ts", "src/main.ts", "src/app.ts", "index.ts"] {
            let full_path = project_path.join(path);
            if full_path.exists() {
                entry_points.push(path.to_string());
            }
        }

        entry_points
    }

    fn identify_key_types(types: &[Symbol]) -> Vec<TypeSummary> {
        let mut type_summaries: Vec<TypeSummary> = types
            .iter()
            .map(|t| TypeSummary {
                name: t.name.clone(),
                kind: t.kind.as_str().to_string(),
                file: t.location.file_path.clone(),
                method_count: 0, // Would need cross-reference to count
                is_key: Self::is_key_type(&t.name, &t.location.file_path),
            })
            .collect();

        // Sort by importance (key types first)
        type_summaries.sort_by(|a, b| b.is_key.cmp(&a.is_key));

        // Limit to top 20
        type_summaries.truncate(20);
        type_summaries
    }

    fn is_key_type(name: &str, file: &str) -> bool {
        // Heuristics for key types:
        // - Types with common important names
        // - Types in core/lib files
        let important_names = [
            "App", "Application", "Config", "Configuration", "Context",
            "Service", "Repository", "Controller", "Handler", "Manager",
            "Client", "Server", "Database", "Connection", "Session",
            "User", "Error", "Result", "State", "Store",
        ];

        let name_lower = name.to_lowercase();
        for important in important_names {
            if name.contains(important) || name_lower.contains(&important.to_lowercase()) {
                return true;
            }
        }

        // Core files are more important
        let important_paths = ["lib.rs", "main.rs", "mod.rs", "index.ts", "app."];
        for path in important_paths {
            if file.contains(path) {
                return true;
            }
        }

        false
    }

    fn identify_key_functions(functions: &[Symbol]) -> Vec<FunctionSummary> {
        let mut summaries: Vec<FunctionSummary> = functions
            .iter()
            .filter(|f| {
                // Filter to public functions and special functions
                f.visibility
                    .as_ref()
                    .map(|v| v.as_str() == "public")
                    .unwrap_or(false)
                    || f.name == "main"
                    || f.name == "new"
                    || f.name.starts_with("create")
                    || f.name.starts_with("init")
            })
            .map(|f| FunctionSummary {
                name: f.name.clone(),
                file: f.location.file_path.clone(),
                signature: f.signature.clone(),
                is_public: f
                    .visibility
                    .as_ref()
                    .map(|v| v.as_str() == "public")
                    .unwrap_or(false),
            })
            .collect();

        // Limit to top 30
        summaries.truncate(30);
        summaries
    }

    fn identify_modules(types: &[Symbol], functions: &[Symbol]) -> Vec<ModuleSummary> {
        // Group by directory
        let mut modules: HashMap<String, usize> = HashMap::new();

        for t in types {
            let dir = Path::new(&t.location.file_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            *modules.entry(dir).or_default() += 1;
        }

        for f in functions {
            let dir = Path::new(&f.location.file_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            *modules.entry(dir).or_default() += 1;
        }

        let mut summaries: Vec<ModuleSummary> = modules
            .into_iter()
            .map(|(path, count)| {
                let name = Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.clone());

                ModuleSummary {
                    name,
                    path,
                    symbol_count: count,
                    purpose: None,
                }
            })
            .collect();

        // Sort by symbol count
        summaries.sort_by(|a, b| b.symbol_count.cmp(&a.symbol_count));
        summaries.truncate(15);
        summaries
    }

    fn detect_patterns(types: &[Symbol], _functions: &[Symbol]) -> Vec<String> {
        let mut patterns = Vec::new();

        let type_names: Vec<&str> = types.iter().map(|t| t.name.as_str()).collect();

        // Repository pattern
        if type_names.iter().any(|n| n.contains("Repository")) {
            patterns.push("Repository Pattern".to_string());
        }

        // Service pattern
        if type_names.iter().any(|n| n.contains("Service")) {
            patterns.push("Service Layer".to_string());
        }

        // Factory pattern
        if type_names.iter().any(|n| n.contains("Factory")) {
            patterns.push("Factory Pattern".to_string());
        }

        // Builder pattern
        if type_names.iter().any(|n| n.contains("Builder")) {
            patterns.push("Builder Pattern".to_string());
        }

        // MVC/Controller pattern
        if type_names.iter().any(|n| n.contains("Controller")) {
            patterns.push("MVC/Controller Pattern".to_string());
        }

        // Handler pattern
        if type_names.iter().any(|n| n.contains("Handler")) {
            patterns.push("Handler Pattern".to_string());
        }

        patterns
    }

    fn detect_conventions<I: CodeIndex>(
        project_path: &Path,
        _index: &I,
    ) -> Result<CodeConventions> {
        let mut conventions = CodeConventions::default();

        // Analyze Cargo.toml for Rust conventions
        let cargo_toml = project_path.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                conventions = Self::detect_rust_conventions(&content, conventions);
            }
        }

        // Analyze package.json for JS/TS conventions
        let package_json = project_path.join("package.json");
        if package_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&package_json) {
                conventions = Self::detect_js_conventions(&content, conventions);
            }
        }

        // Analyze build.gradle for Java/Kotlin conventions
        let build_gradle = project_path.join("build.gradle.kts");
        let build_gradle_groovy = project_path.join("build.gradle");
        if build_gradle.exists() {
            if let Ok(content) = std::fs::read_to_string(&build_gradle) {
                conventions = Self::detect_jvm_conventions(&content, conventions);
            }
        } else if build_gradle_groovy.exists() {
            if let Ok(content) = std::fs::read_to_string(&build_gradle_groovy) {
                conventions = Self::detect_jvm_conventions(&content, conventions);
            }
        }

        Ok(conventions)
    }

    fn detect_rust_conventions(content: &str, mut conventions: CodeConventions) -> CodeConventions {
        // Error handling
        if content.contains("thiserror") {
            conventions.error_handling = Some("thiserror + Result<T>".to_string());
        } else if content.contains("anyhow") {
            conventions.error_handling = Some("anyhow".to_string());
        }

        // Async runtime
        if content.contains("tokio") {
            conventions.async_runtime = Some("tokio".to_string());
        } else if content.contains("async-std") {
            conventions.async_runtime = Some("async-std".to_string());
        }

        // Serialization
        if content.contains("serde") {
            conventions.serialization = Some("serde".to_string());
        }

        // Logging
        if content.contains("tracing") {
            conventions.logging = Some("tracing".to_string());
        } else if content.contains("log") || content.contains("env_logger") {
            conventions.logging = Some("log".to_string());
        }

        // HTTP
        if content.contains("axum") {
            conventions.http_framework = Some("axum".to_string());
        } else if content.contains("actix-web") {
            conventions.http_framework = Some("actix-web".to_string());
        } else if content.contains("warp") {
            conventions.http_framework = Some("warp".to_string());
        }

        // Database
        if content.contains("sqlx") {
            conventions.database = Some("sqlx".to_string());
        } else if content.contains("diesel") {
            conventions.database = Some("diesel".to_string());
        } else if content.contains("rusqlite") {
            conventions.database = Some("rusqlite".to_string());
        }

        // Naming conventions for Rust
        conventions.naming = NamingConventions {
            functions: Some("snake_case".to_string()),
            types: Some("PascalCase".to_string()),
            constants: Some("SCREAMING_SNAKE_CASE".to_string()),
        };

        conventions
    }

    fn detect_js_conventions(content: &str, mut conventions: CodeConventions) -> CodeConventions {
        // Testing
        if content.contains("jest") {
            conventions.testing_framework = Some("jest".to_string());
        } else if content.contains("vitest") {
            conventions.testing_framework = Some("vitest".to_string());
        } else if content.contains("mocha") {
            conventions.testing_framework = Some("mocha".to_string());
        }

        // HTTP frameworks
        if content.contains("express") {
            conventions.http_framework = Some("express".to_string());
        } else if content.contains("fastify") {
            conventions.http_framework = Some("fastify".to_string());
        } else if content.contains("next") {
            conventions.http_framework = Some("next.js".to_string());
        }

        // Naming conventions for JS/TS
        conventions.naming = NamingConventions {
            functions: Some("camelCase".to_string()),
            types: Some("PascalCase".to_string()),
            constants: Some("SCREAMING_SNAKE_CASE".to_string()),
        };

        conventions
    }

    fn detect_jvm_conventions(content: &str, mut conventions: CodeConventions) -> CodeConventions {
        // Testing
        if content.contains("junit") || content.contains("JUnit") {
            conventions.testing_framework = Some("JUnit".to_string());
        }

        // HTTP frameworks
        if content.contains("spring") || content.contains("Spring") {
            conventions.http_framework = Some("Spring Boot".to_string());
        } else if content.contains("ktor") {
            conventions.http_framework = Some("Ktor".to_string());
        }

        // Naming conventions for Java/Kotlin
        conventions.naming = NamingConventions {
            functions: Some("camelCase".to_string()),
            types: Some("PascalCase".to_string()),
            constants: Some("SCREAMING_SNAKE_CASE".to_string()),
        };

        conventions
    }

    fn find_important_files(project_path: &Path) -> Vec<String> {
        let mut files = Vec::new();

        let candidates = [
            "README.md",
            "README",
            "CLAUDE.md",
            "AGENTS.md",
            "Cargo.toml",
            "package.json",
            "build.gradle.kts",
            "build.gradle",
            "pom.xml",
            "src/lib.rs",
            "src/main.rs",
            "src/index.ts",
            "tsconfig.json",
            ".env.example",
            "docker-compose.yml",
            "Dockerfile",
        ];

        for candidate in candidates {
            if project_path.join(candidate).exists() {
                files.push(candidate.to_string());
            }
        }

        files
    }

    fn extract_description(project_path: &Path) -> Option<String> {
        // Try README
        for readme in ["README.md", "README.rst", "README"] {
            let readme_path = project_path.join(readme);
            if readme_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&readme_path) {
                    // Get first non-empty paragraph
                    let lines: Vec<&str> = content.lines().collect();
                    let mut description = String::new();

                    for line in lines {
                        let trimmed = line.trim();
                        if trimmed.is_empty() && !description.is_empty() {
                            break;
                        }
                        if !trimmed.starts_with('#') && !trimmed.is_empty() {
                            if !description.is_empty() {
                                description.push(' ');
                            }
                            description.push_str(trimmed);
                        }
                    }

                    if !description.is_empty() {
                        return Some(description.chars().take(500).collect());
                    }
                }
            }
        }

        // Try Cargo.toml description
        let cargo_toml = project_path.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if let Ok(doc) = content.parse::<toml::Value>() {
                    if let Some(desc) = doc
                        .get("package")
                        .and_then(|p| p.get("description"))
                        .and_then(|d| d.as_str())
                    {
                        return Some(desc.to_string());
                    }
                }
            }
        }

        // Try package.json description
        let package_json = project_path.join("package.json");
        if package_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&package_json) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(desc) = json.get("description").and_then(|d| d.as_str()) {
                        return Some(desc.to_string());
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_rust_conventions() {
        let content = r#"
[package]
name = "test"

[dependencies]
tokio = "1"
serde = "1"
thiserror = "1"
tracing = "0.1"
axum = "0.6"
sqlx = "0.7"
"#;

        let conventions = ArchitectureAnalyzer::detect_rust_conventions(content, CodeConventions::default());

        assert_eq!(conventions.async_runtime, Some("tokio".to_string()));
        assert_eq!(conventions.serialization, Some("serde".to_string()));
        assert_eq!(conventions.error_handling, Some("thiserror + Result<T>".to_string()));
        assert_eq!(conventions.logging, Some("tracing".to_string()));
        assert_eq!(conventions.http_framework, Some("axum".to_string()));
        assert_eq!(conventions.database, Some("sqlx".to_string()));
    }

    #[test]
    fn test_find_important_files() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join("README.md"), "# Test").unwrap();
        fs::write(temp_dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        fs::write(temp_dir.path().join("src/lib.rs"), "").unwrap();

        let files = ArchitectureAnalyzer::find_important_files(temp_dir.path());

        assert!(files.contains(&"README.md".to_string()));
        assert!(files.contains(&"Cargo.toml".to_string()));
        assert!(files.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn test_is_key_type() {
        assert!(ArchitectureAnalyzer::is_key_type("UserService", "src/service.rs"));
        assert!(ArchitectureAnalyzer::is_key_type("AppConfig", "src/config.rs"));
        assert!(ArchitectureAnalyzer::is_key_type("Handler", "src/lib.rs"));
        assert!(!ArchitectureAnalyzer::is_key_type("Helper", "src/utils.rs"));
    }
}
