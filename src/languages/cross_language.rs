//! Cross-language analysis for multi-language projects.
//!
//! This module provides analysis of cross-language references,
//! particularly for Java-Kotlin interop in JVM projects.

use std::collections::HashMap;

use crate::error::Result;
use crate::index::{CodeIndex, SearchOptions, Symbol, SymbolKind};

/// Analyzer for cross-language references
pub struct CrossLanguageAnalyzer {
    /// Enable Java-Kotlin interop analysis
    java_kotlin_interop: bool,
}

/// A cross-language reference
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrossLanguageRef {
    /// Source symbol (the one that references)
    pub source_symbol: String,
    /// Source language
    pub source_language: String,
    /// Source file
    pub source_file: String,
    /// Target symbol (the one being referenced)
    pub target_symbol: String,
    /// Target language
    pub target_language: String,
    /// Target file
    pub target_file: String,
    /// Type of reference
    pub reference_type: CrossRefType,
}

/// Type of cross-language reference
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CrossRefType {
    /// Extends/inherits from a type in another language
    Extends,
    /// Implements an interface from another language
    Implements,
    /// Uses a type from another language
    Uses,
    /// Calls a function/method from another language
    Calls,
    /// Kotlin extension function on a Java type
    ExtensionFunction,
}

impl CrossRefType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CrossRefType::Extends => "extends",
            CrossRefType::Implements => "implements",
            CrossRefType::Uses => "uses",
            CrossRefType::Calls => "calls",
            CrossRefType::ExtensionFunction => "extension_function",
        }
    }
}

impl CrossLanguageAnalyzer {
    /// Create a new cross-language analyzer
    pub fn new() -> Self {
        Self {
            java_kotlin_interop: true,
        }
    }

    /// Find cross-language references for a symbol
    pub fn find_cross_language_refs<I: CodeIndex>(
        &self,
        index: &I,
        symbol_name: &str,
        source_language: Option<&str>,
        target_language: Option<&str>,
    ) -> Result<Vec<CrossLanguageRef>> {
        let mut refs = Vec::new();

        // Find the symbol in all languages
        let definitions = index.find_definition(symbol_name)?;

        // Group definitions by language
        let mut by_language: HashMap<String, Vec<&Symbol>> = HashMap::new();
        for def in &definitions {
            by_language
                .entry(def.language.clone())
                .or_default()
                .push(def);
        }

        // If source language is specified, filter
        let source_langs: Vec<&String> = if let Some(lang) = source_language {
            by_language.keys().filter(|l| *l == lang).collect()
        } else {
            by_language.keys().collect()
        };

        // Find types that might reference this symbol in other languages
        for source_lang in &source_langs {
            let source_defs = by_language.get(*source_lang).unwrap();

            // Look for references in other languages
            for (target_lang, _) in &by_language {
                if *source_lang == target_lang {
                    continue;
                }

                // Skip if target language is filtered
                if let Some(tl) = target_language {
                    if target_lang != tl {
                        continue;
                    }
                }

                // For Java-Kotlin interop
                if self.java_kotlin_interop
                    && ((*source_lang == "java" && target_lang == "kotlin")
                        || (*source_lang == "kotlin" && target_lang == "java"))
                {
                    // Find potential inheritance relationships
                    let potential_refs =
                        self.find_jvm_cross_refs(index, source_defs, source_lang, target_lang)?;
                    refs.extend(potential_refs);
                }
            }
        }

        Ok(refs)
    }

    /// Find Kotlin extensions for a Java type
    pub fn find_kotlin_extensions<I: CodeIndex>(
        &self,
        index: &I,
        java_type: &str,
    ) -> Result<Vec<Symbol>> {
        // Look for Kotlin functions that might be extensions on this type
        let options = SearchOptions {
            language_filter: Some(vec!["kotlin".to_string()]),
            kind_filter: Some(vec![SymbolKind::Function, SymbolKind::Method]),
            ..Default::default()
        };

        let functions = index.list_functions(&options)?;

        // Filter to functions that might be extensions on this type
        // Extension functions in Kotlin have signature like "Type.functionName"
        let extensions: Vec<Symbol> = functions
            .into_iter()
            .filter(|f| {
                f.signature
                    .as_ref()
                    .map(|sig| sig.contains(&format!("{}.", java_type)))
                    .unwrap_or(false)
            })
            .collect();

        Ok(extensions)
    }

    /// Find Java equivalent of a Kotlin type
    pub fn find_java_equivalent<I: CodeIndex>(
        &self,
        index: &I,
        kotlin_type: &str,
    ) -> Result<Option<Symbol>> {
        // Common Kotlin to Java type mappings
        let java_name = match kotlin_type {
            "Int" => "Integer",
            "Long" => "Long",
            "Short" => "Short",
            "Byte" => "Byte",
            "Float" => "Float",
            "Double" => "Double",
            "Boolean" => "Boolean",
            "Char" => "Character",
            "Unit" => return Ok(None), // void has no Java equivalent type
            "Any" => "Object",
            "Nothing" => return Ok(None), // Nothing has no Java equivalent
            _ => kotlin_type,
        };

        // Try to find the Java type
        let definitions = index.find_definition(java_name)?;
        let java_def = definitions.into_iter().find(|d| d.language == "java");

        Ok(java_def)
    }

    fn find_jvm_cross_refs<I: CodeIndex>(
        &self,
        index: &I,
        source_defs: &[&Symbol],
        source_lang: &str,
        target_lang: &str,
    ) -> Result<Vec<CrossLanguageRef>> {
        let mut refs = Vec::new();

        for source_def in source_defs {
            // Look for types in target language that might extend/implement this
            let options = SearchOptions {
                language_filter: Some(vec![target_lang.to_string()]),
                kind_filter: Some(vec![
                    SymbolKind::Class,
                    SymbolKind::Interface,
                    SymbolKind::Trait,
                ]),
                ..Default::default()
            };

            let target_types = index.list_types(&options)?;

            // Check for inheritance by looking at doc comments and signatures
            // (A more complete implementation would use the references table)
            for target_type in target_types {
                let is_related = target_type
                    .signature
                    .as_ref()
                    .map(|sig| sig.contains(&source_def.name))
                    .unwrap_or(false)
                    || target_type
                        .doc_comment
                        .as_ref()
                        .map(|doc| doc.contains(&source_def.name))
                        .unwrap_or(false);

                if is_related {
                    let ref_type = if matches!(source_def.kind, SymbolKind::Interface | SymbolKind::Trait)
                    {
                        CrossRefType::Implements
                    } else {
                        CrossRefType::Extends
                    };

                    refs.push(CrossLanguageRef {
                        source_symbol: source_def.name.clone(),
                        source_language: source_lang.to_string(),
                        source_file: source_def.location.file_path.clone(),
                        target_symbol: target_type.name.clone(),
                        target_language: target_lang.to_string(),
                        target_file: target_type.location.file_path.clone(),
                        reference_type: ref_type,
                    });
                }
            }

            // For Kotlin source, look for extension functions
            if source_lang == "java" && target_lang == "kotlin" {
                let extensions = self.find_kotlin_extensions(index, &source_def.name)?;
                for ext in extensions {
                    refs.push(CrossLanguageRef {
                        source_symbol: source_def.name.clone(),
                        source_language: source_lang.to_string(),
                        source_file: source_def.location.file_path.clone(),
                        target_symbol: ext.name.clone(),
                        target_language: target_lang.to_string(),
                        target_file: ext.location.file_path.clone(),
                        reference_type: CrossRefType::ExtensionFunction,
                    });
                }
            }
        }

        Ok(refs)
    }
}

impl Default for CrossLanguageAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_ref_type_as_str() {
        assert_eq!(CrossRefType::Extends.as_str(), "extends");
        assert_eq!(CrossRefType::Implements.as_str(), "implements");
        assert_eq!(CrossRefType::Uses.as_str(), "uses");
        assert_eq!(CrossRefType::Calls.as_str(), "calls");
        assert_eq!(CrossRefType::ExtensionFunction.as_str(), "extension_function");
    }

    #[test]
    fn test_analyzer_new() {
        let analyzer = CrossLanguageAnalyzer::new();
        assert!(analyzer.java_kotlin_interop);
    }

    #[test]
    fn test_analyzer_default() {
        let analyzer = CrossLanguageAnalyzer::default();
        assert!(analyzer.java_kotlin_interop);
    }
}
