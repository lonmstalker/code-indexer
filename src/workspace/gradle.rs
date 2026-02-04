//! Gradle multi-project build parsing.

use std::path::Path;

use crate::error::{IndexerError, Result};

use super::{ModuleInfo, ModuleType, WorkspaceInfo, WorkspaceType};

/// Parse a Gradle multi-project build
pub fn parse_gradle_workspace(path: &Path) -> Result<WorkspaceInfo> {
    let mut workspace = WorkspaceInfo::new(path.to_path_buf(), WorkspaceType::GradleMultiProject);

    // Try settings.gradle.kts first, then settings.gradle
    let settings_path = if path.join("settings.gradle.kts").exists() {
        path.join("settings.gradle.kts")
    } else {
        path.join("settings.gradle")
    };

    if !settings_path.exists() {
        return Err(IndexerError::FileNotFound(
            "settings.gradle(.kts) not found".to_string(),
        ));
    }

    let content = std::fs::read_to_string(&settings_path).map_err(|e| {
        IndexerError::FileNotFound(format!("Failed to read settings.gradle: {}", e))
    })?;

    // Parse rootProject.name
    if let Some(name) = parse_root_project_name(&content) {
        workspace.name = Some(name);
    }

    // Parse included projects
    let modules = parse_included_projects(&content, path);
    workspace.modules = modules;

    Ok(workspace)
}

fn parse_root_project_name(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();

        // rootProject.name = 'my-project' (Groovy)
        if let Some(rest) = trimmed.strip_prefix("rootProject.name") {
            let name = rest
                .trim()
                .trim_start_matches('=')
                .trim()
                .trim_matches(|c| c == '"' || c == '\'');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn parse_included_projects(content: &str, root: &Path) -> Vec<ModuleInfo> {
    let mut modules = Vec::new();
    let mut project_names = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // include ':app', ':lib:core' (Groovy)
        // include(":app", ":lib:core") (Kotlin DSL)
        if trimmed.starts_with("include") {
            // Extract project names from the line
            let names = extract_project_names(trimmed);
            project_names.extend(names);
        }
    }

    // Create module info for each project
    for name in &project_names {
        // Convert :lib:core to lib/core path
        let path_str = name.trim_start_matches(':').replace(':', "/");
        let module_path = root.join(&path_str);

        if module_path.exists() {
            let module_name = name.trim_start_matches(':').replace(':', "-");
            let mut module = ModuleInfo::new(module_name, path_str.into());

            // Detect language
            if has_kotlin_files(&module_path) {
                module.language = Some("kotlin".to_string());
            } else if has_java_files(&module_path) {
                module.language = Some("java".to_string());
            }

            // Determine module type from build.gradle
            if let Some(module_type) = detect_gradle_module_type(&module_path) {
                module.module_type = Some(module_type);
            }

            // Parse internal dependencies from build.gradle
            let internal_deps = parse_gradle_dependencies(&module_path, &project_names);
            if !internal_deps.is_empty() {
                module.internal_dependencies = internal_deps;
            }

            modules.push(module);
        }
    }

    modules
}

fn extract_project_names(line: &str) -> Vec<String> {
    let mut names = Vec::new();

    // Find all quoted strings in the line
    let mut chars = line.chars().peekable();
    let mut in_quote = false;
    let mut quote_char = '"';
    let mut current = String::new();

    while let Some(c) = chars.next() {
        if !in_quote && (c == '"' || c == '\'') {
            in_quote = true;
            quote_char = c;
            current.clear();
        } else if in_quote && c == quote_char {
            in_quote = false;
            if current.starts_with(':') {
                names.push(current.clone());
            }
        } else if in_quote {
            current.push(c);
        }
    }

    names
}

fn has_kotlin_files(path: &Path) -> bool {
    let src_main = path.join("src/main/kotlin");
    if src_main.exists() {
        return true;
    }

    // Check for .kt files in common locations
    for dir in ["src/main/kotlin", "src/main/java", "src"] {
        let check_path = path.join(dir);
        if check_path.exists() {
            if let Ok(entries) = std::fs::read_dir(&check_path) {
                for entry in entries.flatten() {
                    if entry.path().extension().map_or(false, |e| e == "kt" || e == "kts") {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn has_java_files(path: &Path) -> bool {
    let src_main = path.join("src/main/java");
    if src_main.exists() {
        return true;
    }

    // Check for .java files
    for dir in ["src/main/java", "src"] {
        let check_path = path.join(dir);
        if check_path.exists() {
            if let Ok(entries) = std::fs::read_dir(&check_path) {
                for entry in entries.flatten() {
                    if entry.path().extension().map_or(false, |e| e == "java") {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn detect_gradle_module_type(module_path: &Path) -> Option<ModuleType> {
    // Check build.gradle.kts first, then build.gradle
    let build_file = if module_path.join("build.gradle.kts").exists() {
        module_path.join("build.gradle.kts")
    } else if module_path.join("build.gradle").exists() {
        module_path.join("build.gradle")
    } else {
        return None;
    };

    let content = std::fs::read_to_string(&build_file).ok()?;

    // Check for common plugins
    if content.contains("application") || content.contains("'application'") {
        return Some(ModuleType::Application);
    }

    if content.contains("java-library") || content.contains("'java-library'") {
        return Some(ModuleType::Library);
    }

    if content.contains("kotlin(\"jvm\")") || content.contains("'org.jetbrains.kotlin.jvm'") {
        return Some(ModuleType::Library);
    }

    if content.contains("java-test-fixtures") {
        return Some(ModuleType::Test);
    }

    if content.contains("java-platform") || content.contains("'java-platform'") {
        return Some(ModuleType::Platform);
    }

    // Default to library if has main source
    if module_path.join("src/main").exists() {
        return Some(ModuleType::Library);
    }

    None
}

fn parse_gradle_dependencies(module_path: &Path, all_projects: &[String]) -> Vec<String> {
    let mut deps = Vec::new();

    let build_file = if module_path.join("build.gradle.kts").exists() {
        module_path.join("build.gradle.kts")
    } else if module_path.join("build.gradle").exists() {
        module_path.join("build.gradle")
    } else {
        return deps;
    };

    let content = match std::fs::read_to_string(&build_file) {
        Ok(c) => c,
        Err(_) => return deps,
    };

    // Look for project dependencies: project(":lib:core") or project(':lib:core')
    for line in content.lines() {
        if line.contains("project(") {
            // Extract project reference
            if let Some(start) = line.find("project(") {
                let rest = &line[start + 8..];
                if let Some(end) = rest.find(')') {
                    let proj_ref = rest[..end]
                        .trim()
                        .trim_matches(|c| c == '"' || c == '\'');

                    // Check if this is one of our projects
                    for proj in all_projects {
                        if proj == proj_ref || proj.ends_with(proj_ref) {
                            let dep_name = proj.trim_start_matches(':').replace(':', "-");
                            if !deps.contains(&dep_name) {
                                deps.push(dep_name);
                            }
                        }
                    }
                }
            }
        }
    }

    deps
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_gradle_workspace(temp_dir: &TempDir) {
        // Create settings.gradle.kts
        fs::write(
            temp_dir.path().join("settings.gradle.kts"),
            r#"
rootProject.name = "my-multiproject"

include(":app")
include(":lib:core")
include(":lib:utils")
"#,
        )
        .unwrap();

        // Create app module
        let app_path = temp_dir.path().join("app");
        fs::create_dir_all(app_path.join("src/main/kotlin")).unwrap();
        fs::write(
            app_path.join("build.gradle.kts"),
            r#"
plugins {
    application
    kotlin("jvm")
}

dependencies {
    implementation(project(":lib:core"))
}
"#,
        )
        .unwrap();
        fs::write(app_path.join("src/main/kotlin/Main.kt"), "fun main() {}").unwrap();

        // Create lib:core module
        let core_path = temp_dir.path().join("lib/core");
        fs::create_dir_all(core_path.join("src/main/kotlin")).unwrap();
        fs::write(
            core_path.join("build.gradle.kts"),
            r#"
plugins {
    `java-library`
    kotlin("jvm")
}
"#,
        )
        .unwrap();

        // Create lib:utils module (Java)
        let utils_path = temp_dir.path().join("lib/utils");
        fs::create_dir_all(utils_path.join("src/main/java")).unwrap();
        fs::write(
            utils_path.join("build.gradle.kts"),
            r#"
plugins {
    `java-library`
}
"#,
        )
        .unwrap();
    }

    #[test]
    fn test_parse_gradle_workspace() {
        let temp_dir = TempDir::new().unwrap();
        create_gradle_workspace(&temp_dir);

        let workspace = parse_gradle_workspace(temp_dir.path()).unwrap();

        assert_eq!(workspace.workspace_type, WorkspaceType::GradleMultiProject);
        assert_eq!(workspace.name, Some("my-multiproject".to_string()));
        assert_eq!(workspace.modules.len(), 3);

        let app = workspace.get_module("app").unwrap();
        assert_eq!(app.language, Some("kotlin".to_string()));
        assert_eq!(app.module_type, Some(ModuleType::Application));
        assert!(app.internal_dependencies.contains(&"lib-core".to_string()));

        let core = workspace.get_module("lib-core").unwrap();
        assert_eq!(core.language, Some("kotlin".to_string()));
        assert_eq!(core.module_type, Some(ModuleType::Library));

        let utils = workspace.get_module("lib-utils").unwrap();
        assert_eq!(utils.language, Some("java".to_string()));
    }

    #[test]
    fn test_parse_groovy_settings() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("settings.gradle"),
            r#"
rootProject.name = 'groovy-project'

include ':module-a', ':module-b'
"#,
        )
        .unwrap();

        // Create modules
        for name in ["module-a", "module-b"] {
            let mod_path = temp_dir.path().join(name);
            fs::create_dir_all(mod_path.join("src/main/java")).unwrap();
            fs::write(mod_path.join("build.gradle"), "").unwrap();
        }

        let workspace = parse_gradle_workspace(temp_dir.path()).unwrap();

        assert_eq!(workspace.name, Some("groovy-project".to_string()));
        assert_eq!(workspace.modules.len(), 2);
    }

    #[test]
    fn test_extract_project_names() {
        let names = extract_project_names("include(':app', ':lib')");
        assert_eq!(names, vec![":app", ":lib"]);

        let names = extract_project_names("include(\":app\", \":lib:core\")");
        assert_eq!(names, vec![":app", ":lib:core"]);
    }
}
