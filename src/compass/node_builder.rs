//! Module Node Builder
//!
//! Builds a hierarchical tree of project modules/directories for navigation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::index::CodeIndex;

/// Type of project node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    /// A logical module (Rust mod, TypeScript module, etc.)
    Module,
    /// A directory containing source files
    Directory,
    /// A workspace member/package
    Package,
    /// An architectural layer (api, domain, infra)
    Layer,
}

impl NodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeType::Module => "module",
            NodeType::Directory => "directory",
            NodeType::Package => "package",
            NodeType::Layer => "layer",
        }
    }
}

/// A node in the project hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectNode {
    /// Node ID (e.g., "mod:src/api" or "dir:src")
    pub id: String,
    /// Parent node ID
    pub parent_id: Option<String>,
    /// Node type
    pub node_type: NodeType,
    /// Display name
    pub name: String,
    /// File system path
    pub path: String,
    /// Total symbol count in this node
    pub symbol_count: usize,
    /// Public symbol count
    pub public_symbol_count: usize,
    /// File count
    pub file_count: usize,
    /// Centrality score (how connected this module is)
    pub centrality_score: f32,
    /// Children node IDs
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
}

/// Builder for project node hierarchy
pub struct NodeBuilder;

impl NodeBuilder {
    /// Build the project node hierarchy from the index
    pub fn build(index: &dyn CodeIndex, root_path: &str) -> crate::error::Result<Vec<ProjectNode>> {
        let stats = index.get_stats()?;
        let mut nodes = Vec::new();
        let mut dir_stats: HashMap<String, (usize, usize, usize)> = HashMap::new(); // (symbol_count, public_count, file_count)

        // Aggregate stats by directory
        let options = crate::index::SearchOptions {
            limit: Some(10000),
            ..Default::default()
        };

        // Get all symbols and group by directory
        if let Ok(functions) = index.list_functions(&options) {
            for sym in functions {
                let dir = Path::new(&sym.location.file_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                let entry = dir_stats.entry(dir).or_insert((0, 0, 0));
                entry.0 += 1;
                if sym.visibility == Some(crate::index::Visibility::Public) {
                    entry.1 += 1;
                }
            }
        }

        if let Ok(types) = index.list_types(&options) {
            for sym in types {
                let dir = Path::new(&sym.location.file_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                let entry = dir_stats.entry(dir).or_insert((0, 0, 0));
                entry.0 += 1;
                if sym.visibility == Some(crate::index::Visibility::Public) {
                    entry.1 += 1;
                }
            }
        }

        // Count files per directory
        for (lang, count) in &stats.files_by_language {
            // This is approximate - we don't have per-directory file counts
            // So we distribute proportionally
            let _ = lang;
            let _ = count;
        }

        // Build nodes from directories
        let root = Path::new(root_path);

        for (dir_path, (symbol_count, public_count, file_count)) in &dir_stats {
            let rel_path = Path::new(dir_path)
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| dir_path.clone());

            if rel_path.is_empty() {
                continue;
            }

            let name = Path::new(&rel_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| rel_path.clone());

            let parent_path = Path::new(&rel_path)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(|p| format!("dir:{}", p.to_string_lossy()));

            let node_type = Self::infer_node_type(&rel_path, &name);

            let node = ProjectNode {
                id: format!("dir:{}", rel_path),
                parent_id: parent_path,
                node_type,
                name,
                path: rel_path.clone(),
                symbol_count: *symbol_count,
                public_symbol_count: *public_count,
                file_count: *file_count,
                centrality_score: 0.0, // Will be computed later
                children: Vec::new(),
            };

            nodes.push(node);
        }

        // Build parent-child relationships
        // First collect the parent-child mapping
        let mut children_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for node in &nodes {
            if let Some(ref parent_id) = node.parent_id {
                children_map.entry(parent_id.clone())
                    .or_default()
                    .push(node.id.clone());
            }
        }

        // Then apply the mapping
        for node in &mut nodes {
            if let Some(children) = children_map.remove(&node.id) {
                node.children = children;
            }
        }

        // Sort by symbol count descending
        nodes.sort_by(|a, b| b.symbol_count.cmp(&a.symbol_count));

        Ok(nodes)
    }

    fn infer_node_type(path: &str, name: &str) -> NodeType {
        let lower_name = name.to_lowercase();
        let lower_path = path.to_lowercase();

        // Check for layer patterns
        if matches!(lower_name.as_str(),
            "api" | "rest" | "graphql" | "grpc" | "handlers" | "controllers" | "routes"
        ) {
            return NodeType::Layer;
        }

        if matches!(lower_name.as_str(),
            "domain" | "core" | "models" | "entities" | "services"
        ) {
            return NodeType::Layer;
        }

        if matches!(lower_name.as_str(),
            "infra" | "infrastructure" | "adapters" | "repositories" | "db" | "database"
        ) {
            return NodeType::Layer;
        }

        // Check for module patterns
        if lower_path.contains("src/") || lower_path.starts_with("src") {
            return NodeType::Module;
        }

        // Check for package patterns
        if lower_path.contains("packages/") || lower_path.contains("crates/") {
            return NodeType::Package;
        }

        NodeType::Directory
    }

    /// Get top-level nodes (those without parents in the node list)
    pub fn get_top_level(nodes: &[ProjectNode]) -> Vec<&ProjectNode> {
        let all_ids: std::collections::HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();

        nodes.iter()
            .filter(|n| {
                n.parent_id.as_ref()
                    .map(|p| !all_ids.contains(p.as_str()))
                    .unwrap_or(true)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_type_inference() {
        assert_eq!(NodeBuilder::infer_node_type("src/api", "api"), NodeType::Layer);
        assert_eq!(NodeBuilder::infer_node_type("src/domain", "domain"), NodeType::Layer);
        assert_eq!(NodeBuilder::infer_node_type("src/utils", "utils"), NodeType::Module);
        // "core" matches layer pattern, so it's Layer not Package
        assert_eq!(NodeBuilder::infer_node_type("packages/core", "core"), NodeType::Layer);
        // Test actual package detection
        assert_eq!(NodeBuilder::infer_node_type("packages/mylib", "mylib"), NodeType::Package);
    }

    #[test]
    fn test_node_type_as_str() {
        assert_eq!(NodeType::Module.as_str(), "module");
        assert_eq!(NodeType::Layer.as_str(), "layer");
    }
}
