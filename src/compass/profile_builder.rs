//! Project Profile Builder
//!
//! Builds a profile of the project including languages, frameworks, and build tools.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::index::CodeIndex;

/// Statistics for a programming language in the project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageStats {
    /// Language name
    pub name: String,
    /// Number of files
    pub file_count: usize,
    /// Number of symbols
    pub symbol_count: usize,
    /// Percentage of codebase
    pub percentage: f32,
}

/// Information about a detected framework
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkInfo {
    /// Framework name
    pub name: String,
    /// Framework category (web, cli, testing, etc.)
    pub category: String,
    /// Evidence of detection (file or pattern found)
    pub evidence: String,
}

/// Complete project profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProfile {
    /// Language statistics
    pub languages: Vec<LanguageStats>,
    /// Detected frameworks
    pub frameworks: Vec<FrameworkInfo>,
    /// Detected build tools
    pub build_tools: Vec<String>,
    /// Workspace type (cargo, npm, gradle, etc.)
    pub workspace_type: Option<String>,
    /// Total file count
    pub total_files: usize,
    /// Total symbol count
    pub total_symbols: usize,
}

/// Builder for project profiles
pub struct ProfileBuilder;

impl ProfileBuilder {
    /// Build a project profile from the index
    pub fn build(index: &dyn CodeIndex) -> crate::error::Result<ProjectProfile> {
        let stats = index.get_stats()?;

        // Aggregate language stats
        let mut lang_map: HashMap<String, (usize, usize)> = HashMap::new();

        for (lang, count) in &stats.symbols_by_language {
            let entry = lang_map.entry(lang.clone()).or_insert((0, 0));
            entry.1 += *count;
        }

        for (lang, count) in &stats.files_by_language {
            let entry = lang_map.entry(lang.clone()).or_insert((0, 0));
            entry.0 += *count;
        }

        let total_symbols: usize = lang_map.values().map(|(_, s)| s).sum();

        let mut languages: Vec<LanguageStats> = lang_map
            .into_iter()
            .map(|(name, (file_count, symbol_count))| {
                let percentage = if total_symbols > 0 {
                    (symbol_count as f32 / total_symbols as f32) * 100.0
                } else {
                    0.0
                };
                LanguageStats {
                    name,
                    file_count,
                    symbol_count,
                    percentage,
                }
            })
            .collect();

        // Sort by symbol count descending
        languages.sort_by(|a, b| b.symbol_count.cmp(&a.symbol_count));

        // Detect frameworks based on symbols and patterns
        let frameworks = Self::detect_frameworks(index)?;

        // Detect build tools
        let build_tools = Self::detect_build_tools(index)?;

        // Detect workspace type
        let workspace_type = Self::detect_workspace_type(&languages, &build_tools);

        Ok(ProjectProfile {
            languages,
            frameworks,
            build_tools,
            workspace_type,
            total_files: stats.total_files,
            total_symbols,
        })
    }

    fn detect_frameworks(index: &dyn CodeIndex) -> crate::error::Result<Vec<FrameworkInfo>> {
        let mut frameworks = Vec::new();

        // Rust frameworks
        let rust_patterns = [
            ("actix", "actix_web", "web", "import of actix_web"),
            ("axum", "axum", "web", "import of axum"),
            ("rocket", "rocket", "web", "import of rocket"),
            ("warp", "warp", "web", "import of warp"),
            ("tokio", "tokio", "async runtime", "import of tokio"),
            ("clap", "clap", "cli", "import of clap"),
            ("serde", "serde", "serialization", "import of serde"),
        ];

        // JavaScript/TypeScript frameworks
        let js_patterns = [
            ("React", "React", "web", "import of React"),
            ("Vue", "Vue", "web", "import of Vue"),
            ("Angular", "angular", "web", "import of angular"),
            ("Express", "express", "web", "import of express"),
            ("Next.js", "next", "web", "import of next"),
            ("Jest", "jest", "testing", "import of jest"),
            ("Mocha", "mocha", "testing", "import of mocha"),
        ];

        // Python frameworks
        let py_patterns = [
            ("Django", "django", "web", "import of django"),
            ("Flask", "flask", "web", "import of flask"),
            ("FastAPI", "fastapi", "web", "import of fastapi"),
            ("pytest", "pytest", "testing", "import of pytest"),
        ];

        // Java/Kotlin frameworks
        let java_patterns = [
            ("Spring", "springframework", "web", "import of spring"),
            ("Spring Boot", "SpringBootApplication", "web", "SpringBootApplication annotation"),
            ("JUnit", "junit", "testing", "import of junit"),
        ];

        // Check all patterns
        for (name, pattern, category, evidence) in rust_patterns
            .iter()
            .chain(js_patterns.iter())
            .chain(py_patterns.iter())
            .chain(java_patterns.iter())
        {
            let options = crate::index::SearchOptions {
                limit: Some(1),
                ..Default::default()
            };

            if let Ok(results) = index.search(pattern, &options) {
                if !results.is_empty() {
                    frameworks.push(FrameworkInfo {
                        name: name.to_string(),
                        category: category.to_string(),
                        evidence: evidence.to_string(),
                    });
                }
            }
        }

        // Deduplicate by name
        frameworks.sort_by(|a, b| a.name.cmp(&b.name));
        frameworks.dedup_by(|a, b| a.name == b.name);

        Ok(frameworks)
    }

    fn detect_build_tools(index: &dyn CodeIndex) -> crate::error::Result<Vec<String>> {
        let mut tools = Vec::new();

        // Check config digests
        if let Ok(configs) = index.get_all_config_digests() {
            for config in configs {
                match config.config_type {
                    crate::docs::ConfigType::CargoToml => {
                        if !tools.contains(&"cargo".to_string()) {
                            tools.push("cargo".to_string());
                        }
                    }
                    crate::docs::ConfigType::PackageJson => {
                        if !tools.contains(&"npm".to_string()) {
                            tools.push("npm".to_string());
                        }
                        // Check for specific tools in scripts
                        if config.scripts.keys().any(|k| k.contains("yarn")) {
                            if !tools.contains(&"yarn".to_string()) {
                                tools.push("yarn".to_string());
                            }
                        }
                    }
                    crate::docs::ConfigType::Makefile => {
                        if !tools.contains(&"make".to_string()) {
                            tools.push("make".to_string());
                        }
                    }
                    crate::docs::ConfigType::PyProjectToml => {
                        if !tools.contains(&"pip".to_string()) {
                            tools.push("pip".to_string());
                        }
                    }
                    crate::docs::ConfigType::GoMod => {
                        if !tools.contains(&"go".to_string()) {
                            tools.push("go".to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(tools)
    }

    fn detect_workspace_type(languages: &[LanguageStats], build_tools: &[String]) -> Option<String> {
        // Determine primary language
        let primary = languages.first()?;

        // Map language to workspace type
        match primary.name.to_lowercase().as_str() {
            "rust" => Some("cargo".to_string()),
            "javascript" | "typescript" => {
                if build_tools.contains(&"yarn".to_string()) {
                    Some("yarn".to_string())
                } else {
                    Some("npm".to_string())
                }
            }
            "python" => Some("python".to_string()),
            "go" => Some("go".to_string()),
            "java" | "kotlin" => {
                if build_tools.contains(&"gradle".to_string()) {
                    Some("gradle".to_string())
                } else {
                    Some("maven".to_string())
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_stats_percentage() {
        let stats = LanguageStats {
            name: "rust".to_string(),
            file_count: 10,
            symbol_count: 100,
            percentage: 50.0,
        };

        assert_eq!(stats.name, "rust");
        assert_eq!(stats.percentage, 50.0);
    }
}
