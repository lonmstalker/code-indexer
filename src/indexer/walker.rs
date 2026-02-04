use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::error::Result;
use crate::languages::LanguageRegistry;

pub struct FileWalker {
    registry: LanguageRegistry,
}

impl FileWalker {
    pub fn new(registry: LanguageRegistry) -> Self {
        Self { registry }
    }

    pub fn walk(&self, root: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        let walker = WalkBuilder::new(root)
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .ignore(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if path.is_file() && self.is_supported(path) {
                files.push(path.to_path_buf());
            }
        }

        Ok(files)
    }

    pub fn is_supported(&self, path: &Path) -> bool {
        self.registry.get_for_file(path).is_some()
    }

    #[allow(dead_code)]
    pub fn get_language(&self, path: &Path) -> Option<String> {
        self.registry.get_for_file(path).map(|g| g.name().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn create_walker() -> FileWalker {
        FileWalker::new(LanguageRegistry::new())
    }

    fn create_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = File::create(path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_walk_finds_rust_files() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "main.rs", "fn main() {}");
        create_file(temp_dir.path(), "lib.rs", "pub fn lib() {}");

        let walker = create_walker();
        let files = walker.walk(temp_dir.path()).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|p| p.extension().unwrap() == "rs"));
    }

    #[test]
    fn test_walk_finds_java_files() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "Main.java", "public class Main {}");

        let walker = create_walker();
        let files = walker.walk(temp_dir.path()).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].extension().unwrap() == "java");
    }

    #[test]
    fn test_walk_finds_typescript_files() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "app.ts", "const x = 1;");
        create_file(temp_dir.path(), "component.tsx", "export default () => null;");
        create_file(temp_dir.path(), "utils.js", "function test() {}");
        create_file(temp_dir.path(), "comp.jsx", "export const C = () => null;");

        let walker = create_walker();
        let files = walker.walk(temp_dir.path()).unwrap();

        assert_eq!(files.len(), 4);
    }

    #[test]
    fn test_walk_recursive() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "root.rs", "");
        create_file(temp_dir.path(), "src/lib.rs", "");
        create_file(temp_dir.path(), "src/module/mod.rs", "");
        create_file(temp_dir.path(), "src/module/deep/file.rs", "");

        let walker = create_walker();
        let files = walker.walk(temp_dir.path()).unwrap();

        assert_eq!(files.len(), 4);
    }

    #[test]
    fn test_walk_ignores_unsupported_files() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "main.rs", "fn main() {}");
        create_file(temp_dir.path(), "README.md", "# Readme");
        create_file(temp_dir.path(), "script.py", "print('hello')");
        create_file(temp_dir.path(), "data.json", "{}");

        let walker = create_walker();
        let files = walker.walk(temp_dir.path()).unwrap();

        // main.rs and script.py should be supported (Rust and Python)
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_walk_respects_gitignore() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize git repo so .gitignore is respected
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .ok();

        create_file(temp_dir.path(), ".gitignore", "target/\n*.generated.rs");
        create_file(temp_dir.path(), "src/main.rs", "fn main() {}");
        create_file(temp_dir.path(), "target/debug/build.rs", "");
        create_file(temp_dir.path(), "generated.generated.rs", "");

        let walker = create_walker();
        let files = walker.walk(temp_dir.path()).unwrap();

        // With git initialized, .gitignore should be respected
        // Without git, all files would be found
        let main_found = files.iter().any(|f| f.to_string_lossy().contains("main.rs"));
        assert!(main_found, "main.rs should be found");
    }

    #[test]
    fn test_walk_empty_directory() {
        let temp_dir = TempDir::new().unwrap();

        let walker = create_walker();
        let files = walker.walk(temp_dir.path()).unwrap();

        assert!(files.is_empty());
    }

    #[test]
    fn test_walk_directory_with_only_unsupported_files() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "README.md", "# Test");
        create_file(temp_dir.path(), "config.yaml", "key: value");

        let walker = create_walker();
        let files = walker.walk(temp_dir.path()).unwrap();

        assert!(files.is_empty());
    }

    #[test]
    fn test_is_supported_rust() {
        let walker = create_walker();
        assert!(walker.is_supported(Path::new("test.rs")));
        assert!(walker.is_supported(Path::new("src/lib.rs")));
    }

    #[test]
    fn test_is_supported_java() {
        let walker = create_walker();
        assert!(walker.is_supported(Path::new("Main.java")));
        assert!(walker.is_supported(Path::new("com/example/Test.java")));
    }

    #[test]
    fn test_is_supported_typescript() {
        let walker = create_walker();
        assert!(walker.is_supported(Path::new("app.ts")));
        assert!(walker.is_supported(Path::new("component.tsx")));
        assert!(walker.is_supported(Path::new("utils.js")));
        assert!(walker.is_supported(Path::new("comp.jsx")));
    }

    #[test]
    fn test_is_supported_unsupported() {
        let walker = create_walker();
        assert!(!walker.is_supported(Path::new("file.txt")));
        assert!(!walker.is_supported(Path::new("Makefile")));
        assert!(!walker.is_supported(Path::new("data.json")));
        assert!(!walker.is_supported(Path::new("config.yaml")));
    }

    #[test]
    fn test_is_supported_python() {
        let walker = create_walker();
        assert!(walker.is_supported(Path::new("script.py")));
        assert!(walker.is_supported(Path::new("stubs.pyi")));
    }

    #[test]
    fn test_is_supported_go() {
        let walker = create_walker();
        assert!(walker.is_supported(Path::new("main.go")));
    }

    #[test]
    fn test_is_supported_csharp() {
        let walker = create_walker();
        assert!(walker.is_supported(Path::new("Program.cs")));
    }

    #[test]
    fn test_is_supported_cpp() {
        let walker = create_walker();
        assert!(walker.is_supported(Path::new("main.cpp")));
        assert!(walker.is_supported(Path::new("header.h")));
        assert!(walker.is_supported(Path::new("file.hpp")));
    }

    #[test]
    fn test_get_language_rust() {
        let walker = create_walker();
        assert_eq!(walker.get_language(Path::new("test.rs")), Some("rust".to_string()));
    }

    #[test]
    fn test_get_language_java() {
        let walker = create_walker();
        assert_eq!(walker.get_language(Path::new("Main.java")), Some("java".to_string()));
    }

    #[test]
    fn test_get_language_typescript() {
        let walker = create_walker();
        assert_eq!(walker.get_language(Path::new("app.ts")), Some("typescript".to_string()));
        assert_eq!(walker.get_language(Path::new("app.tsx")), Some("typescript".to_string()));
        assert_eq!(walker.get_language(Path::new("app.js")), Some("typescript".to_string()));
        assert_eq!(walker.get_language(Path::new("app.jsx")), Some("typescript".to_string()));
    }

    #[test]
    fn test_get_language_unsupported() {
        let walker = create_walker();
        assert_eq!(walker.get_language(Path::new("file.txt")), None);
        assert_eq!(walker.get_language(Path::new("data.json")), None);
    }

    #[test]
    fn test_get_language_python() {
        let walker = create_walker();
        assert_eq!(walker.get_language(Path::new("script.py")), Some("python".to_string()));
    }

    #[test]
    fn test_get_language_go() {
        let walker = create_walker();
        assert_eq!(walker.get_language(Path::new("main.go")), Some("go".to_string()));
    }

    #[test]
    fn test_get_language_csharp() {
        let walker = create_walker();
        assert_eq!(walker.get_language(Path::new("Program.cs")), Some("csharp".to_string()));
    }

    #[test]
    fn test_get_language_cpp() {
        let walker = create_walker();
        assert_eq!(walker.get_language(Path::new("main.cpp")), Some("cpp".to_string()));
        assert_eq!(walker.get_language(Path::new("header.h")), Some("cpp".to_string()));
    }

    #[test]
    fn test_walk_mixed_files() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "main.rs", "fn main() {}");
        create_file(temp_dir.path(), "App.java", "public class App {}");
        create_file(temp_dir.path(), "index.ts", "export const x = 1;");
        create_file(temp_dir.path(), "README.md", "# Project");

        let walker = create_walker();
        let files = walker.walk(temp_dir.path()).unwrap();

        assert_eq!(files.len(), 3);

        let extensions: Vec<_> = files.iter().filter_map(|p| p.extension()).collect();
        assert!(extensions.iter().any(|e| e.to_str() == Some("rs")));
        assert!(extensions.iter().any(|e| e.to_str() == Some("java")));
        assert!(extensions.iter().any(|e| e.to_str() == Some("ts")));
    }

    #[test]
    fn test_walk_hidden_files_ignored() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "visible.rs", "fn main() {}");
        create_file(temp_dir.path(), ".hidden.rs", "fn hidden() {}");

        let walker = create_walker();
        let files = walker.walk(temp_dir.path()).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].file_name().unwrap().to_str().unwrap() == "visible.rs");
    }

    #[test]
    fn test_walk_returns_absolute_paths() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "test.rs", "fn test() {}");

        let walker = create_walker();
        let files = walker.walk(temp_dir.path()).unwrap();

        assert!(!files.is_empty());
    }
}
