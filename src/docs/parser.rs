//! Markdown documentation parser
//!
//! Extracts headings, code blocks, and key sections from README and other markdown files.

use serde::{Deserialize, Serialize};

/// Type of documentation file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocType {
    Readme,
    Contributing,
    Changelog,
    License,
    Other,
}

impl DocType {
    pub fn from_filename(filename: &str) -> Self {
        let lower = filename.to_lowercase();
        if lower.starts_with("readme") {
            DocType::Readme
        } else if lower.starts_with("contributing") {
            DocType::Contributing
        } else if lower.starts_with("changelog") || lower.starts_with("history") {
            DocType::Changelog
        } else if lower.starts_with("license") {
            DocType::License
        } else {
            DocType::Other
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            DocType::Readme => "readme",
            DocType::Contributing => "contributing",
            DocType::Changelog => "changelog",
            DocType::License => "license",
            DocType::Other => "other",
        }
    }
}

/// A heading in a markdown document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heading {
    /// Heading level (1-6)
    pub level: u8,
    /// Heading text
    pub text: String,
    /// Line number (1-based)
    pub line: u32,
}

/// A fenced code block in markdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    /// Language hint (e.g., "bash", "rust", "json")
    pub language: Option<String>,
    /// Code content
    pub content: String,
    /// Starting line number (1-based)
    pub line: u32,
}

/// A key section identified by common headings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeySection {
    /// Section heading
    pub heading: String,
    /// Start line (1-based)
    pub start_line: u32,
    /// End line (1-based, exclusive)
    pub end_line: u32,
}

/// Parsed documentation digest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocDigest {
    /// File path
    pub file_path: String,
    /// Document type
    pub doc_type: DocType,
    /// Document title (first H1)
    pub title: Option<String>,
    /// All headings
    pub headings: Vec<Heading>,
    /// Command/code blocks (filtered to likely commands)
    pub command_blocks: Vec<CodeBlock>,
    /// Key sections (installation, usage, etc.)
    pub key_sections: Vec<KeySection>,
}

/// Parser for markdown documentation
pub struct DocParser;

impl DocParser {
    /// Parse a markdown file and extract structured information
    pub fn parse(file_path: &str, content: &str) -> DocDigest {
        let filename = std::path::Path::new(file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let doc_type = DocType::from_filename(&filename);
        let lines: Vec<&str> = content.lines().collect();

        let headings = Self::extract_headings(&lines);
        let title = headings.iter()
            .find(|h| h.level == 1)
            .map(|h| h.text.clone());

        let all_code_blocks = Self::extract_code_blocks(&lines);
        let command_blocks = Self::filter_command_blocks(all_code_blocks);

        let key_sections = Self::identify_key_sections(&headings, lines.len() as u32);

        DocDigest {
            file_path: file_path.to_string(),
            doc_type,
            title,
            headings,
            command_blocks,
            key_sections,
        }
    }

    fn extract_headings(lines: &[&str]) -> Vec<Heading> {
        let mut headings = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                let level = trimmed.chars().take_while(|&c| c == '#').count();
                if level <= 6 {
                    let text = trimmed[level..].trim_start_matches(' ').to_string();
                    if !text.is_empty() {
                        headings.push(Heading {
                            level: level as u8,
                            text,
                            line: (i + 1) as u32,
                        });
                    }
                }
            }
        }

        headings
    }

    fn extract_code_blocks(lines: &[&str]) -> Vec<CodeBlock> {
        let mut blocks = Vec::new();
        let mut in_block = false;
        let mut block_start = 0u32;
        let mut block_lang: Option<String> = None;
        let mut block_content = String::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            if trimmed.starts_with("```") {
                if in_block {
                    // End of block
                    blocks.push(CodeBlock {
                        language: block_lang.take(),
                        content: block_content.trim_end().to_string(),
                        line: block_start,
                    });
                    block_content.clear();
                    in_block = false;
                } else {
                    // Start of block
                    in_block = true;
                    block_start = (i + 1) as u32;
                    let lang = trimmed[3..].trim();
                    block_lang = if lang.is_empty() {
                        None
                    } else {
                        Some(lang.split_whitespace().next().unwrap_or("").to_string())
                    };
                }
            } else if in_block {
                if !block_content.is_empty() {
                    block_content.push('\n');
                }
                block_content.push_str(line);
            }
        }

        blocks
    }

    fn filter_command_blocks(blocks: Vec<CodeBlock>) -> Vec<CodeBlock> {
        blocks.into_iter()
            .filter(|b| Self::is_command_block(b))
            .collect()
    }

    fn is_command_block(block: &CodeBlock) -> bool {
        // Check language hint
        if let Some(ref lang) = block.language {
            let lang_lower = lang.to_lowercase();
            if matches!(lang_lower.as_str(),
                "bash" | "sh" | "shell" | "zsh" | "console" | "terminal" |
                "cmd" | "powershell" | "ps1"
            ) {
                return true;
            }
        }

        // Check content for command patterns
        let content = &block.content;
        let first_line = content.lines().next().unwrap_or("");

        // Common command prefixes
        if first_line.starts_with("$ ") ||
           first_line.starts_with("> ") ||
           first_line.starts_with("npm ") ||
           first_line.starts_with("yarn ") ||
           first_line.starts_with("pnpm ") ||
           first_line.starts_with("cargo ") ||
           first_line.starts_with("make ") ||
           first_line.starts_with("docker ") ||
           first_line.starts_with("git ") ||
           first_line.starts_with("pip ") ||
           first_line.starts_with("python ") ||
           first_line.starts_with("go ") {
            return true;
        }

        false
    }

    fn identify_key_sections(headings: &[Heading], total_lines: u32) -> Vec<KeySection> {
        let key_patterns = [
            "install", "installation", "getting started", "setup", "quick start",
            "usage", "how to use", "examples", "example",
            "api", "api reference", "reference",
            "configuration", "config", "options",
            "contributing", "development", "building", "build",
            "testing", "tests", "test",
            "license", "changelog", "changes",
            "requirements", "prerequisites", "dependencies",
        ];

        let mut sections = Vec::new();

        for (i, heading) in headings.iter().enumerate() {
            let lower_text = heading.text.to_lowercase();
            let is_key = key_patterns.iter().any(|p| lower_text.contains(p));

            if is_key {
                // Find end: next heading of same or higher level, or end of doc
                let end_line = headings.iter()
                    .skip(i + 1)
                    .find(|h| h.level <= heading.level)
                    .map(|h| h.line)
                    .unwrap_or(total_lines + 1);

                sections.push(KeySection {
                    heading: heading.text.clone(),
                    start_line: heading.line,
                    end_line,
                });
            }
        }

        sections
    }

    /// Extract a specific section by heading name
    pub fn extract_section(content: &str, heading: &str) -> Option<String> {
        let lines: Vec<&str> = content.lines().collect();
        let headings = Self::extract_headings(&lines);

        let lower_heading = heading.to_lowercase();

        // Find the heading
        let (idx, found) = headings.iter()
            .enumerate()
            .find(|(_, h)| h.text.to_lowercase().contains(&lower_heading))?;

        // Find the end
        let end_line = headings.iter()
            .skip(idx + 1)
            .find(|h| h.level <= found.level)
            .map(|h| (h.line - 1) as usize)
            .unwrap_or(lines.len());

        let start_line = found.line as usize; // Skip the heading itself

        if start_line < lines.len() {
            Some(lines[start_line..end_line].join("\n").trim().to_string())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_readme() {
        let content = r#"# My Project

Some intro text.

## Installation

```bash
npm install my-project
```

## Usage

```javascript
const proj = require('my-project');
```

### Advanced Usage

More content here.

## License

MIT
"#;

        let digest = DocParser::parse("README.md", content);

        assert_eq!(digest.doc_type, DocType::Readme);
        assert_eq!(digest.title.as_deref(), Some("My Project"));
        assert_eq!(digest.headings.len(), 5);
        assert_eq!(digest.command_blocks.len(), 1);
        assert_eq!(digest.command_blocks[0].language.as_deref(), Some("bash"));
        assert_eq!(digest.key_sections.len(), 4); // Installation, Usage, Advanced Usage, License
    }

    #[test]
    fn test_extract_section() {
        let content = r#"# Project

## Installation

Run this command:

```bash
cargo install project
```

## Usage

Use it like this.
"#;

        let section = DocParser::extract_section(content, "installation");
        assert!(section.is_some());
        assert!(section.unwrap().contains("cargo install"));
    }

    #[test]
    fn test_command_block_detection() {
        let bash_block = CodeBlock {
            language: Some("bash".to_string()),
            content: "echo hello".to_string(),
            line: 1,
        };
        assert!(DocParser::is_command_block(&bash_block));

        let npm_block = CodeBlock {
            language: None,
            content: "npm install foo".to_string(),
            line: 1,
        };
        assert!(DocParser::is_command_block(&npm_block));

        let rust_block = CodeBlock {
            language: Some("rust".to_string()),
            content: "fn main() {}".to_string(),
            line: 1,
        };
        assert!(!DocParser::is_command_block(&rust_block));
    }
}
