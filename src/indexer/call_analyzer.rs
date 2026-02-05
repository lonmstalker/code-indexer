//! Call Analyzer for determining call graph confidence levels
//!
//! This module analyzes function calls to determine whether the call target
//! is certain (statically known) or possible (may be different at runtime).

use tree_sitter::Node;

use crate::error::Result;
use crate::index::{
    CallConfidence, CallGraphEdge, CodeIndex, Scope, SymbolKind, UncertaintyReason,
};

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

        match methods.len() {
            0 => Ok(CallAnalysisResult {
                callee_name: format!("{}.{}", receiver, method_name),
                callee_id: None,
                confidence: CallConfidence::Possible,
                reason: Some(UncertaintyReason::DynamicReceiver),
            }),
            1 => Ok(CallAnalysisResult {
                callee_name: method_name.to_string(),
                callee_id: Some(methods[0].id.clone()),
                confidence: CallConfidence::Certain,
                reason: None,
            }),
            _ => Ok(CallAnalysisResult {
                callee_name: method_name.to_string(),
                callee_id: Some(methods[0].id.clone()),
                confidence: CallConfidence::Possible,
                reason: Some(UncertaintyReason::MultipleCandidates),
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
        _caller_scope: &Scope,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        if let Some(function) = call_node.child_by_field_name("function") {
            let func_text = &source[function.byte_range()];

            // Python is dynamically typed
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

    /// Analyzes a Go call expression
    fn analyze_go_call(
        &self,
        call_node: &Node,
        source: &str,
        _caller_scope: &Scope,
        index: &dyn CodeIndex,
    ) -> Result<CallAnalysisResult> {
        if let Some(function) = call_node.child_by_field_name("function") {
            let func_text = &source[function.byte_range()];

            // Check for selector expression (pkg.Func or obj.Method)
            if function.kind() == "selector_expression" {
                if let Some(field) = function.child_by_field_name("field") {
                    let method_name = &source[field.byte_range()];
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

                    return Ok(CallAnalysisResult {
                        callee_name: method_name.to_string(),
                        callee_id: methods.first().map(|s| s.id.clone()),
                        confidence: if methods.len() == 1 {
                            CallConfidence::Certain
                        } else {
                            CallConfidence::Possible
                        },
                        reason: if methods.len() > 1 {
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

    #[test]
    fn test_call_analysis_result_unresolved() {
        let result = CallAnalysisResult::unresolved("unknown_func");
        assert_eq!(result.callee_name, "unknown_func");
        assert!(result.callee_id.is_none());
        assert_eq!(result.confidence, CallConfidence::Possible);
        assert_eq!(result.reason, Some(UncertaintyReason::DynamicReceiver));
    }
}
