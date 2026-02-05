//! Call Analyzer for determining call graph confidence levels
//!
//! This module analyzes function calls to determine whether the call target
//! is certain (statically known) or possible (may be different at runtime).

use tree_sitter::Node;

use crate::error::Result;
use crate::index::{
    CallConfidence, CallGraphEdge, CodeIndex, Scope, ScopeKind, SymbolKind, UncertaintyReason,
};

/// Extracts receiver type from Go method name patterns like "(*Server).Start" or "Server.Start"
fn extract_go_receiver_type(method_name: &str) -> Option<String> {
    // Pattern: (*Type).Method or Type.Method
    if let Some(paren_start) = method_name.find('(') {
        if let Some(paren_end) = method_name.find(')') {
            let inner = &method_name[paren_start + 1..paren_end];
            // Remove pointer prefix if present
            let type_name = inner.trim_start_matches('*');
            if !type_name.is_empty() {
                return Some(type_name.to_string());
            }
        }
    }
    // Pattern: Type.Method
    if let Some(dot_pos) = method_name.find('.') {
        let type_part = &method_name[..dot_pos];
        if !type_part.is_empty() && type_part.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            return Some(type_part.to_string());
        }
    }
    None
}

/// Analyzer for determining call graph edge confidence
pub struct CallAnalyzer;

impl Default for CallAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl CallAnalyzer {
    /// Creates a new call analyzer
    pub fn new() -> Self {
        Self
    }

    /// Filter candidates by receiver type.
    /// If receiver_type matches a candidate's parent, that candidate is preferred.
    /// Returns (best_match, is_certain) where is_certain is true if exactly one match found.
    fn filter_by_type<'a>(
        candidates: &'a [crate::index::Symbol],
        receiver_type: Option<&str>,
    ) -> (Option<&'a crate::index::Symbol>, bool) {
        if candidates.is_empty() {
            return (None, false);
        }

        if candidates.len() == 1 {
            return (Some(&candidates[0]), true);
        }

        // If we have a receiver type, try to match it to parent
        if let Some(rt) = receiver_type {
            let rt_lower = rt.to_lowercase();
            let matches: Vec<_> = candidates
                .iter()
                .filter(|s| {
                    s.parent
                        .as_ref()
                        .map(|p| p.to_lowercase().contains(&rt_lower) || rt_lower.contains(&p.to_lowercase()))
                        .unwrap_or(false)
                })
                .collect();

            if matches.len() == 1 {
                return (Some(matches[0]), true);
            } else if !matches.is_empty() {
                // Multiple matches but at least filtered down
                return (Some(matches[0]), false);
            }
        }

        // No type filtering possible, return first with uncertainty
        (Some(&candidates[0]), false)
    }

    /// Analyzes a call expression and determines confidence level
    pub fn analyze_call(
        &self,
        call_node: &Node,
        source: &str,
        caller_scope: &Scope,
        index: &dyn CodeIndex,
        language: &str,
    ) -> Result<CallAnalysisResult> {
        match language {
            "rust" => self.analyze_rust_call(call_node, source, caller_scope, index),
            "java" | "kotlin" => self.analyze_java_call(call_node, source, caller_scope, index),
            "typescript" | "javascript" | "tsx" => {
                self.analyze_ts_call(call_node, source, caller_scope, index)
            }
            "python" => self.analyze_python_call(call_node, source, caller_scope, index),
            "go" => self.analyze_go_call(call_node, source, caller_scope, index),
            _ => self.analyze_generic_call(call_node, source, caller_scope, index),
        }
    }

    /// Analyzes a Rust call expression
    fn analyze_rust_call(
        &self,
        call_node: &Node,
        source: &str,
        _caller_scope: &Scope,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        let kind = call_node.kind();

        match kind {
            "call_expression" => {
                // Direct function call: foo()
                if let Some(function) = call_node.child_by_field_name("function") {
                    let func_text = &source[function.byte_range()];
                    return self.resolve_rust_function(func_text, index);
                }
            }
            "method_call_expression" => {
                // Method call: obj.method()
                if let Some(receiver) = call_node.child_by_field_name("receiver") {
                    let receiver_text = &source[receiver.byte_range()];
                    if let Some(method_name) = call_node.child_by_field_name("method") {
                        let method_text = &source[method_name.byte_range()];
                        return self.resolve_rust_method(receiver_text, method_text, index);
                    }
                }
            }
            _ => {}
        }

        Ok(CallAnalysisResult::unresolved("unknown"))
    }

    /// Resolves a Rust function name to its definition
    fn resolve_rust_function(
        &self,
        func_name: &str,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        // Check for qualified paths like module::function
        let parts: Vec<&str> = func_name.split("::").collect();
        let name = parts.last().unwrap_or(&func_name);

        let symbols = index.find_definition(name)?;

        if symbols.is_empty() {
            // Could be external or generic
            return Ok(CallAnalysisResult {
                callee_name: func_name.to_string(),
                callee_id: None,
                confidence: CallConfidence::Possible,
                reason: Some(UncertaintyReason::ExternalLibrary),
            });
        }

        // Filter to functions/methods
        let functions: Vec<_> = symbols
            .into_iter()
            .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
            .collect();

        match functions.len() {
            0 => Ok(CallAnalysisResult {
                callee_name: func_name.to_string(),
                callee_id: None,
                confidence: CallConfidence::Possible,
                reason: Some(UncertaintyReason::ExternalLibrary),
            }),
            1 => Ok(CallAnalysisResult {
                callee_name: func_name.to_string(),
                callee_id: Some(functions[0].id.clone()),
                confidence: CallConfidence::Certain,
                reason: None,
            }),
            _ => Ok(CallAnalysisResult {
                callee_name: func_name.to_string(),
                callee_id: Some(functions[0].id.clone()),
                confidence: CallConfidence::Possible,
                reason: Some(UncertaintyReason::MultipleCandidates),
            }),
        }
    }

    /// Resolves a Rust method call
    fn resolve_rust_method(
        &self,
        receiver: &str,
        method_name: &str,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        // Check if receiver is a known type
        let receiver_clean = receiver.trim();

        // Check for self/Self receiver - method on current type
        if receiver_clean == "self" || receiver_clean == "Self" {
            // This is a certain call to a method in the current impl
            let methods = index.find_definition(method_name)?;
            if methods.len() == 1 {
                return Ok(CallAnalysisResult {
                    callee_name: method_name.to_string(),
                    callee_id: Some(methods[0].id.clone()),
                    confidence: CallConfidence::Certain,
                    reason: None,
                });
            }
            // For self calls, filter by methods only (not functions)
            let method_candidates: Vec<_> = methods
                .iter()
                .filter(|s| s.kind == SymbolKind::Method)
                .collect();
            if method_candidates.len() == 1 {
                return Ok(CallAnalysisResult {
                    callee_name: method_name.to_string(),
                    callee_id: Some(method_candidates[0].id.clone()),
                    confidence: CallConfidence::Certain,
                    reason: None,
                });
            }
        }

        // Check if receiver is a trait object (dyn Trait)
        if receiver_clean.contains("dyn ") || receiver_clean.starts_with("&dyn") {
            return Ok(CallAnalysisResult {
                callee_name: method_name.to_string(),
                callee_id: None,
                confidence: CallConfidence::Possible,
                reason: Some(UncertaintyReason::VirtualDispatch),
            });
        }

        // Try to find the method definition
        let methods = index.find_definition(method_name)?;

        // Try type-aware filtering if receiver looks like a type or variable
        // Check if receiver starts with uppercase (likely a type) or is a known identifier
        let receiver_type = if receiver_clean.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            Some(receiver_clean)
        } else {
            // Could be a variable - we'd need scope analysis to determine its type
            // For now, check if receiver matches any function's first param type (self pattern)
            None
        };

        let (best_match, is_certain) = Self::filter_by_type(&methods, receiver_type);

        match (best_match, is_certain) {
            (Some(m), true) => Ok(CallAnalysisResult {
                callee_name: method_name.to_string(),
                callee_id: Some(m.id.clone()),
                confidence: CallConfidence::Certain,
                reason: None,
            }),
            (Some(m), false) => Ok(CallAnalysisResult {
                callee_name: method_name.to_string(),
                callee_id: Some(m.id.clone()),
                confidence: CallConfidence::Possible,
                reason: Some(UncertaintyReason::MultipleCandidates),
            }),
            (None, _) => Ok(CallAnalysisResult {
                callee_name: format!("{}.{}", receiver, method_name),
                callee_id: None,
                confidence: CallConfidence::Possible,
                reason: Some(UncertaintyReason::DynamicReceiver),
            }),
        }
    }

    /// Analyzes a Java/Kotlin call expression
    fn analyze_java_call(
        &self,
        call_node: &Node,
        source: &str,
        _caller_scope: &Scope,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        let kind = call_node.kind();

        match kind {
            "method_invocation" => {
                if let Some(name) = call_node.child_by_field_name("name") {
                    let method_name = &source[name.byte_range()];

                    // Check for receiver
                    if let Some(object) = call_node.child_by_field_name("object") {
                        let receiver = &source[object.byte_range()];

                        // Check for interface/abstract class calls
                        let methods = index.find_definition(method_name)?;
                        let is_interface = methods
                            .iter()
                            .any(|s| matches!(s.kind, SymbolKind::Interface | SymbolKind::Trait));

                        if is_interface {
                            return Ok(CallAnalysisResult {
                                callee_name: method_name.to_string(),
                                callee_id: methods.first().map(|s| s.id.clone()),
                                confidence: CallConfidence::Possible,
                                reason: Some(UncertaintyReason::VirtualDispatch),
                            });
                        }

                        return self.resolve_java_method(receiver, method_name, index);
                    }

                    // Unqualified call - could be static import or method in current class
                    let methods = index.find_definition(method_name)?;
                    if methods.len() == 1 {
                        return Ok(CallAnalysisResult {
                            callee_name: method_name.to_string(),
                            callee_id: Some(methods[0].id.clone()),
                            confidence: CallConfidence::Certain,
                            reason: None,
                        });
                    }
                }
            }
            _ => {}
        }

        Ok(CallAnalysisResult::unresolved("unknown"))
    }

    fn resolve_java_method(
        &self,
        receiver: &str,
        method_name: &str,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        let methods = index.find_definition(method_name)?;

        if methods.is_empty() {
            return Ok(CallAnalysisResult {
                callee_name: method_name.to_string(),
                callee_id: None,
                confidence: CallConfidence::Possible,
                reason: Some(UncertaintyReason::ExternalLibrary),
            });
        }

        // Use type filtering for multiple candidates
        let receiver_type = if receiver.is_empty() { None } else { Some(receiver) };
        let (best_match, is_certain) = Self::filter_by_type(&methods, receiver_type);

        Ok(CallAnalysisResult {
            callee_name: method_name.to_string(),
            callee_id: best_match.map(|s| s.id.clone()),
            confidence: if is_certain {
                CallConfidence::Certain
            } else {
                CallConfidence::Possible
            },
            reason: if !is_certain && methods.len() > 1 {
                Some(UncertaintyReason::MultipleCandidates)
            } else {
                None
            },
        })
    }

    /// Analyzes a TypeScript/JavaScript call expression
    fn analyze_ts_call(
        &self,
        call_node: &Node,
        source: &str,
        _caller_scope: &Scope,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        let kind = call_node.kind();

        match kind {
            "call_expression" => {
                if let Some(function) = call_node.child_by_field_name("function") {
                    let func_text = &source[function.byte_range()];

                    // Check for member expression (obj.method())
                    if function.kind() == "member_expression" {
                        // Extract receiver (object) for potential type matching
                        let receiver_type = function
                            .child_by_field_name("object")
                            .map(|obj| &source[obj.byte_range()]);

                        if let Some(property) = function.child_by_field_name("property") {
                            let method_name = &source[property.byte_range()];
                            return self.resolve_ts_method(method_name, receiver_type, index);
                        }
                    }

                    return self.resolve_ts_method(func_text, None, index);
                }
            }
            _ => {}
        }

        Ok(CallAnalysisResult::unresolved("unknown"))
    }

    fn resolve_ts_method(
        &self,
        func_name: &str,
        receiver_type: Option<&str>,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        let functions = index.find_definition(func_name)?;

        if functions.is_empty() {
            return Ok(CallAnalysisResult {
                callee_name: func_name.to_string(),
                callee_id: None,
                confidence: CallConfidence::Possible,
                reason: Some(UncertaintyReason::DynamicReceiver),
            });
        }

        // Use type filtering for multiple candidates
        let (best_match, is_certain) = Self::filter_by_type(&functions, receiver_type);

        Ok(CallAnalysisResult {
            callee_name: func_name.to_string(),
            callee_id: best_match.map(|s| s.id.clone()),
            confidence: if is_certain {
                CallConfidence::Certain
            } else {
                CallConfidence::Possible
            },
            reason: if !is_certain && functions.len() > 1 {
                Some(UncertaintyReason::MultipleCandidates)
            } else {
                None
            },
        })
    }

    /// Analyzes a Python call expression
    fn analyze_python_call(
        &self,
        call_node: &Node,
        source: &str,
        caller_scope: &Scope,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        if let Some(function) = call_node.child_by_field_name("function") {
            let func_text = &source[function.byte_range()];

            // Check for method call pattern: obj.method()
            if function.kind() == "attribute" {
                if let Some(attr_name) = function.child_by_field_name("attribute") {
                    let method_name = &source[attr_name.byte_range()];

                    // Get the receiver (object)
                    if let Some(obj) = function.child_by_field_name("object") {
                        let receiver = &source[obj.byte_range()];

                        // Try to infer receiver type from scope's typed parameters
                        let inferred_type = self.infer_python_variable_type(receiver, caller_scope);

                        let methods = index.find_definition(method_name)?;

                        // Use type-aware filtering if we have type info
                        let (best_match, is_certain) = Self::filter_by_type(&methods, inferred_type.as_deref());

                        return Ok(CallAnalysisResult {
                            callee_name: method_name.to_string(),
                            callee_id: best_match.map(|s| s.id.clone()),
                            confidence: if is_certain {
                                CallConfidence::Certain
                            } else {
                                CallConfidence::Possible
                            },
                            reason: if methods.is_empty() {
                                Some(UncertaintyReason::DynamicReceiver)
                            } else if !is_certain && methods.len() > 1 {
                                Some(UncertaintyReason::MultipleCandidates)
                            } else {
                                None
                            },
                        });
                    }
                }
            }

            // Direct function call
            let functions = index.find_definition(func_text)?;

            return Ok(CallAnalysisResult {
                callee_name: func_text.to_string(),
                callee_id: functions.first().map(|s| s.id.clone()),
                confidence: if functions.len() == 1 {
                    CallConfidence::Certain
                } else {
                    CallConfidence::Possible
                },
                reason: if functions.is_empty() {
                    Some(UncertaintyReason::DynamicReceiver)
                } else if functions.len() > 1 {
                    Some(UncertaintyReason::MultipleCandidates)
                } else {
                    None
                },
            });
        }

        Ok(CallAnalysisResult::unresolved("unknown"))
    }

    /// Tries to infer Python variable type from scope's typed parameters
    fn infer_python_variable_type(&self, var_name: &str, scope: &Scope) -> Option<String> {
        // Check if var_name is 'self' - it's the class type
        if var_name == "self" {
            // For methods, the scope kind would be Function inside a Class scope
            // The class name might be derived from scope.name if it's a method
            // For now, check if scope name contains the class pattern
            if scope.kind == ScopeKind::Function {
                // Method names are often "ClassName.method_name" or just "method_name"
                // We'd need parent scope info to get class name
                // Return None for now - would need index lookup by parent_id
                return None;
            }
            if scope.kind == ScopeKind::Class {
                return scope.name.clone();
            }
        }

        // Check function parameters for type annotations
        // The scope might contain the function's parameters with types
        // For now, return None - full implementation would require tracking variable types
        // through scope analysis

        None
    }

    /// Analyzes a Go call expression
    fn analyze_go_call(
        &self,
        call_node: &Node,
        source: &str,
        caller_scope: &Scope,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        if let Some(function) = call_node.child_by_field_name("function") {
            let func_text = &source[function.byte_range()];

            // Check for selector expression (pkg.Func or obj.Method)
            if function.kind() == "selector_expression" {
                if let Some(field) = function.child_by_field_name("field") {
                    let method_name = &source[field.byte_range()];

                    // Get the operand (receiver)
                    let receiver_type = if let Some(operand) = function.child_by_field_name("operand") {
                        let operand_text = &source[operand.byte_range()];
                        self.infer_go_variable_type(operand_text, caller_scope)
                    } else {
                        None
                    };

                    let methods = index.find_definition(method_name)?;

                    // Go interfaces are virtual dispatch
                    let is_interface = methods
                        .iter()
                        .any(|s| s.kind == SymbolKind::Interface);

                    if is_interface {
                        return Ok(CallAnalysisResult {
                            callee_name: method_name.to_string(),
                            callee_id: methods.first().map(|s| s.id.clone()),
                            confidence: CallConfidence::Possible,
                            reason: Some(UncertaintyReason::VirtualDispatch),
                        });
                    }

                    // Use type-aware filtering if we have receiver type info
                    let (best_match, is_certain) = Self::filter_by_type(&methods, receiver_type.as_deref());

                    return Ok(CallAnalysisResult {
                        callee_name: method_name.to_string(),
                        callee_id: best_match.map(|s| s.id.clone()),
                        confidence: if is_certain {
                            CallConfidence::Certain
                        } else {
                            CallConfidence::Possible
                        },
                        reason: if methods.is_empty() {
                            Some(UncertaintyReason::ExternalLibrary)
                        } else if !is_certain && methods.len() > 1 {
                            Some(UncertaintyReason::MultipleCandidates)
                        } else {
                            None
                        },
                    });
                }
            }

            let functions = index.find_definition(func_text)?;
            return Ok(CallAnalysisResult {
                callee_name: func_text.to_string(),
                callee_id: functions.first().map(|s| s.id.clone()),
                confidence: if functions.len() == 1 {
                    CallConfidence::Certain
                } else {
                    CallConfidence::Possible
                },
                reason: if functions.is_empty() {
                    Some(UncertaintyReason::ExternalLibrary)
                } else if functions.len() > 1 {
                    Some(UncertaintyReason::MultipleCandidates)
                } else {
                    None
                },
            });
        }

        Ok(CallAnalysisResult::unresolved("unknown"))
    }

    /// Tries to infer Go variable type from scope or naming conventions
    fn infer_go_variable_type(&self, var_name: &str, scope: &Scope) -> Option<String> {
        // In Go, receivers are typically short (e.g., 's' for Server)
        // Check if scope has parent_id (it's inside something, likely a method)
        if var_name.len() <= 3 && scope.parent_id.is_some() {
            // For Go methods, the scope name might contain the receiver type
            // e.g., "(*Server).Start" or "Server.Start"
            if let Some(ref name) = scope.name {
                // Try to extract type from method receiver pattern
                if let Some(type_name) = extract_go_receiver_type(name) {
                    return Some(type_name);
                }
            }
        }

        // Check if var_name starts with uppercase (it's a type/package, not a variable)
        if var_name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            // This is likely a package or type name, not a receiver
            return Some(var_name.to_string());
        }

        // For other cases, we'd need full type inference
        // which requires tracking variable declarations and their types
        None
    }

    /// Generic call analysis for unsupported languages
    fn analyze_generic_call(
        &self,
        call_node: &Node,
        source: &str,
        _caller_scope: &Scope,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        // Try to extract function name from the call
        let call_text = &source[call_node.byte_range()];

        // Simple heuristic: find the first identifier
        let func_name = call_text
            .split('(')
            .next()
            .unwrap_or("")
            .trim()
            .split('.')
            .last()
            .unwrap_or("");

        if func_name.is_empty() {
            return Ok(CallAnalysisResult::unresolved("unknown"));
        }

        let functions = index.find_definition(func_name)?;

        Ok(CallAnalysisResult {
            callee_name: func_name.to_string(),
            callee_id: functions.first().map(|s| s.id.clone()),
            confidence: if functions.len() == 1 {
                CallConfidence::Certain
            } else {
                CallConfidence::Possible
            },
            reason: if functions.is_empty() {
                Some(UncertaintyReason::ExternalLibrary)
            } else if functions.len() > 1 {
                Some(UncertaintyReason::MultipleCandidates)
            } else {
                None
            },
        })
    }

    /// Converts analysis result to a call graph edge
    pub fn to_edge(
        &self,
        result: &CallAnalysisResult,
        caller_id: &str,
        file_path: &str,
        line: u32,
        column: u32,
    ) -> CallGraphEdge {
        CallGraphEdge {
            from: caller_id.to_string(),
            to: result.callee_id.clone(),
            callee_name: result.callee_name.clone(),
            call_site_file: file_path.to_string(),
            call_site_line: line,
            call_site_column: column,
            confidence: result.confidence.clone(),
            reason: result.reason.clone(),
        }
    }
}

/// Result of call analysis
#[derive(Debug, Clone)]
pub struct CallAnalysisResult {
    /// Name of the called function
    pub callee_name: String,
    /// ID of the resolved callee symbol (if found)
    pub callee_id: Option<String>,
    /// Confidence level
    pub confidence: CallConfidence,
    /// Reason for uncertainty
    pub reason: Option<UncertaintyReason>,
}

impl CallAnalysisResult {
    /// Creates an unresolved result
    pub fn unresolved(name: &str) -> Self {
        Self {
            callee_name: name.to_string(),
            callee_id: None,
            confidence: CallConfidence::Possible,
            reason: Some(UncertaintyReason::DynamicReceiver),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{Location, Symbol};

    #[test]
    fn test_call_analysis_result_unresolved() {
        let result = CallAnalysisResult::unresolved("unknown_func");
        assert_eq!(result.callee_name, "unknown_func");
        assert!(result.callee_id.is_none());
        assert_eq!(result.confidence, CallConfidence::Possible);
        assert_eq!(result.reason, Some(UncertaintyReason::DynamicReceiver));
    }

    #[test]
    fn test_extract_go_receiver_type_pointer() {
        let result = extract_go_receiver_type("(*Server).Start");
        assert_eq!(result, Some("Server".to_string()));
    }

    #[test]
    fn test_extract_go_receiver_type_value() {
        let result = extract_go_receiver_type("Calculator.Add");
        assert_eq!(result, Some("Calculator".to_string()));
    }

    #[test]
    fn test_extract_go_receiver_type_no_match() {
        let result = extract_go_receiver_type("add");
        assert_eq!(result, None);
    }

    fn create_test_symbol(name: &str, parent: Option<&str>) -> Symbol {
        let mut s = Symbol::new(
            name,
            SymbolKind::Method,
            Location::new("/test.rs", 1, 0, 10, 0),
            "rust",
        );
        s.parent = parent.map(|p| p.to_string());
        s
    }

    #[test]
    fn test_filter_by_type_single_candidate() {
        let symbols = vec![create_test_symbol("method", Some("TypeA"))];
        let (best, is_certain) = CallAnalyzer::filter_by_type(&symbols, None);
        assert!(best.is_some());
        assert!(is_certain); // Single candidate is certain
    }

    #[test]
    fn test_filter_by_type_multiple_with_match() {
        let symbols = vec![
            create_test_symbol("method", Some("TypeA")),
            create_test_symbol("method", Some("TypeB")),
        ];
        let (best, is_certain) = CallAnalyzer::filter_by_type(&symbols, Some("TypeA"));
        assert!(best.is_some());
        assert_eq!(best.unwrap().parent.as_deref(), Some("TypeA"));
        assert!(is_certain); // Exact match on type
    }

    #[test]
    fn test_filter_by_type_multiple_no_match() {
        let symbols = vec![
            create_test_symbol("method", Some("TypeA")),
            create_test_symbol("method", Some("TypeB")),
        ];
        let (best, is_certain) = CallAnalyzer::filter_by_type(&symbols, Some("TypeC"));
        assert!(best.is_some()); // Returns first candidate
        assert!(!is_certain); // No type match, uncertain
    }

    #[test]
    fn test_filter_by_type_empty() {
        let symbols: Vec<Symbol> = vec![];
        let (best, is_certain) = CallAnalyzer::filter_by_type(&symbols, Some("TypeA"));
        assert!(best.is_none());
        assert!(!is_certain);
    }
}
