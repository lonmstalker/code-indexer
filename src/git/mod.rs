use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{IndexerError, Result};
use crate::index::{CodeIndex, Symbol};

/// Git integration for tracking changed symbols
pub struct GitAnalyzer {
    repo_path: PathBuf,
}

/// Information about a changed file
#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub status: ChangeStatus,
}

/// Type of change
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

impl ChangeStatus {
    pub fn from_git_status(status: &str) -> Option<Self> {
        match status.chars().next()? {
            'A' => Some(ChangeStatus::Added),
            'M' => Some(ChangeStatus::Modified),
            'D' => Some(ChangeStatus::Deleted),
            'R' => Some(ChangeStatus::Renamed),
            '?' => Some(ChangeStatus::Added), // Untracked
            _ => Some(ChangeStatus::Modified),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ChangeStatus::Added => "added",
            ChangeStatus::Modified => "modified",
            ChangeStatus::Deleted => "deleted",
            ChangeStatus::Renamed => "renamed",
        }
    }
}

/// Symbol with change information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangedSymbol {
    pub symbol: Symbol,
    pub change_status: String,
    pub file_status: String,
}

impl GitAnalyzer {
    pub fn new(repo_path: impl AsRef<Path>) -> Result<Self> {
        let repo_path = repo_path.as_ref().to_path_buf();

        // Verify it's a git repository
        let output = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(&repo_path)
            .output()
            .map_err(|e| IndexerError::Index(format!("Failed to run git: {}", e)))?;

        if !output.status.success() {
            return Err(IndexerError::Index(
                "Not a git repository".to_string(),
            ));
        }

        Ok(Self { repo_path })
    }

    /// Get list of changed files compared to a base reference
    pub fn get_changed_files(&self, base: &str, staged: bool, unstaged: bool) -> Result<Vec<ChangedFile>> {
        let mut files = Vec::new();
        let mut seen = HashSet::new();

        // Get staged changes
        if staged {
            let output = Command::new("git")
                .args(["diff", "--cached", "--name-status", base])
                .current_dir(&self.repo_path)
                .output()
                .map_err(|e| IndexerError::Index(format!("Failed to run git diff: {}", e)))?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Some((status, path)) = Self::parse_diff_line(line) {
                        if seen.insert(path.clone()) {
                            files.push(ChangedFile { path, status });
                        }
                    }
                }
            }
        }

        // Get unstaged changes
        if unstaged {
            let output = Command::new("git")
                .args(["diff", "--name-status"])
                .current_dir(&self.repo_path)
                .output()
                .map_err(|e| IndexerError::Index(format!("Failed to run git diff: {}", e)))?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Some((status, path)) = Self::parse_diff_line(line) {
                        if seen.insert(path.clone()) {
                            files.push(ChangedFile { path, status });
                        }
                    }
                }
            }

            // Also get untracked files
            let output = Command::new("git")
                .args(["ls-files", "--others", "--exclude-standard"])
                .current_dir(&self.repo_path)
                .output()
                .map_err(|e| IndexerError::Index(format!("Failed to run git ls-files: {}", e)))?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let path = line.trim().to_string();
                    if !path.is_empty() && seen.insert(path.clone()) {
                        files.push(ChangedFile {
                            path,
                            status: ChangeStatus::Added,
                        });
                    }
                }
            }
        }

        // If neither staged nor unstaged, get all uncommitted changes
        if !staged && !unstaged {
            let output = Command::new("git")
                .args(["status", "--porcelain"])
                .current_dir(&self.repo_path)
                .output()
                .map_err(|e| IndexerError::Index(format!("Failed to run git status: {}", e)))?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.len() > 3 {
                        let status_str = &line[0..2];
                        let path = line[3..].trim().to_string();

                        let status = if status_str.starts_with('A') || status_str.ends_with('A') || status_str.starts_with('?') {
                            ChangeStatus::Added
                        } else if status_str.starts_with('D') || status_str.ends_with('D') {
                            ChangeStatus::Deleted
                        } else if status_str.starts_with('R') {
                            ChangeStatus::Renamed
                        } else {
                            ChangeStatus::Modified
                        };

                        if seen.insert(path.clone()) {
                            files.push(ChangedFile { path, status });
                        }
                    }
                }
            }
        }

        Ok(files)
    }

    /// Find symbols in changed files
    pub fn find_changed_symbols(
        &self,
        index: &dyn CodeIndex,
        base: &str,
        staged: bool,
        unstaged: bool,
    ) -> Result<Vec<ChangedSymbol>> {
        let changed_files = self.get_changed_files(base, staged, unstaged)?;
        let mut changed_symbols = Vec::new();

        for file in changed_files {
            // Skip deleted files - their symbols are gone
            if file.status == ChangeStatus::Deleted {
                continue;
            }

            // Try multiple path formats since the index might store paths differently
            let paths_to_try = vec![
                file.path.clone(),
                format!("./{}", file.path),
                self.repo_path.join(&file.path).to_string_lossy().to_string(),
            ];

            for file_path in paths_to_try {
                if let Ok(symbols) = index.get_file_symbols(&file_path) {
                    if !symbols.is_empty() {
                        for symbol in symbols {
                            changed_symbols.push(ChangedSymbol {
                                symbol,
                                change_status: "modified".to_string(),
                                file_status: file.status.as_str().to_string(),
                            });
                        }
                        break; // Found symbols, no need to try other path formats
                    }
                }
            }
        }

        Ok(changed_symbols)
    }

    /// Get changed lines for a specific file
    pub fn get_changed_lines(&self, file_path: &str, base: &str) -> Result<Vec<u32>> {
        let output = Command::new("git")
            .args(["diff", "--unified=0", base, "--", file_path])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| IndexerError::Index(format!("Failed to run git diff: {}", e)))?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut lines = Vec::new();

        for line in stdout.lines() {
            // Parse hunk headers like @@ -10,5 +12,7 @@
            if line.starts_with("@@") {
                if let Some(plus_part) = line.split('+').nth(1) {
                    if let Some(range) = plus_part.split_whitespace().next() {
                        let parts: Vec<&str> = range.split(',').collect();
                        if let Ok(start) = parts[0].parse::<u32>() {
                            let count = if parts.len() > 1 {
                                parts[1].parse::<u32>().unwrap_or(1)
                            } else {
                                1
                            };
                            for i in 0..count {
                                lines.push(start + i);
                            }
                        }
                    }
                }
            }
        }

        Ok(lines)
    }

    fn parse_diff_line(line: &str) -> Option<(ChangeStatus, String)> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let status = ChangeStatus::from_git_status(parts[0])?;
            let path = parts.last()?.to_string();
            Some((status, path))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_status_from_git_status() {
        assert_eq!(ChangeStatus::from_git_status("A"), Some(ChangeStatus::Added));
        assert_eq!(ChangeStatus::from_git_status("M"), Some(ChangeStatus::Modified));
        assert_eq!(ChangeStatus::from_git_status("D"), Some(ChangeStatus::Deleted));
        assert_eq!(ChangeStatus::from_git_status("R"), Some(ChangeStatus::Renamed));
        assert_eq!(ChangeStatus::from_git_status("?"), Some(ChangeStatus::Added));
    }

    #[test]
    fn test_parse_diff_line() {
        let (status, path) = GitAnalyzer::parse_diff_line("M\tsrc/main.rs").unwrap();
        assert_eq!(status, ChangeStatus::Modified);
        assert_eq!(path, "src/main.rs");

        let (status, path) = GitAnalyzer::parse_diff_line("A\tnew_file.rs").unwrap();
        assert_eq!(status, ChangeStatus::Added);
        assert_eq!(path, "new_file.rs");
    }
}
