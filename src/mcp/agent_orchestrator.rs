use std::collections::HashSet;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::error::{IndexerError, Result};
use crate::index::NextAction;

use super::agent_client::{AgentClient, AgentMessage, AgentUsage};
use super::consolidated::{
    AgentCollectionMeta, AgentTokenUsage, AgentTraceCall, AgentTraceStep, ContextCoverage,
    CoverageGap, DependencyTouchpoint, DocConfigDigestEntry, FileImportEdge, ModuleDependencyEdge,
    SuggestedToolCall, SymbolInteractionEdge, TaskContextDigest,
};

const DEFAULT_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_MAX_STEPS: u32 = 6;

const ALLOWED_TOOLS: &[&str] = &[
    "search_symbols",
    "find_references",
    "analyze_call_graph",
    "get_file_outline",
    "get_imports",
    "list_modules",
    "find_module_dependencies",
    "get_architecture_summary",
    "get_stats",
    "list_dependencies",
    "get_dependency_info",
    "get_dependency_source",
    "get_doc_section",
    "get_project_compass",
    "get_project_commands",
];

#[derive(Debug, Clone)]
pub struct AgentCollectionRequest {
    pub query: String,
    pub file: Option<String>,
    pub task_hint: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_steps: Option<u32>,
    pub include_trace: bool,
    pub provider: String,
    pub model: Option<String>,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentCollectionResult {
    pub task_context: TaskContextDigest,
    pub coverage: ContextCoverage,
    pub gaps: Vec<CoverageGap>,
    pub collection_meta: AgentCollectionMeta,
    pub next_actions: Vec<NextAction>,
    pub suggested_tool_calls: Vec<SuggestedToolCall>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentStepCommand {
    #[serde(default)]
    done: bool,
    #[serde(default)]
    focus: Option<String>,
    #[serde(default)]
    calls: Vec<AgentToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentToolCall {
    tool: String,
    #[serde(default)]
    args: serde_json::Value,
}

pub async fn run_agent_context_collection<F>(
    client: &AgentClient,
    request: AgentCollectionRequest,
    mut execute_tool: F,
) -> Result<AgentCollectionResult>
where
    F: FnMut(&str, &serde_json::Value) -> std::result::Result<serde_json::Value, String>,
{
    let started = Instant::now();
    let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS).max(1);
    let max_steps = request.max_steps.unwrap_or(DEFAULT_MAX_STEPS).max(1);

    let mut task_context = TaskContextDigest::default();
    let mut observed = LayerObserved::default();
    let mut coverage = compute_coverage(&task_context, &observed);
    let mut gaps: Vec<CoverageGap> = Vec::new();
    let mut trace: Vec<AgentTraceStep> = Vec::new();
    let mut usage_acc = UsageAccumulator::default();
    let mut finish_reason: Option<String> = None;
    let mut suggested_tool_calls: Vec<SuggestedToolCall> = Vec::new();
    let mut suggested_keys: HashSet<String> = HashSet::new();
    let mut steps_taken: u32 = 0;
    let mut timeout_reached = false;
    let mut max_steps_reached = false;

    for step in 1..=max_steps {
        steps_taken = step;
        if started.elapsed().as_millis() as u64 >= timeout_ms {
            timeout_reached = true;
            break;
        }

        let state_payload = serde_json::json!({
            "query": request.query,
            "file": request.file,
            "task_hint": request.task_hint,
            "coverage": coverage,
            "collected_counts": {
                "module_graph": task_context.module_graph.len(),
                "file_import_graph": task_context.file_import_graph.len(),
                "symbol_interactions": task_context.symbol_interactions.len(),
                "deps_touchpoints": task_context.deps_touchpoints.len(),
                "docs_config_digest": task_context.docs_config_digest.len(),
            },
            "recent_gaps": gaps.iter().rev().take(5).cloned().collect::<Vec<_>>(),
            "allowed_tools": ALLOWED_TOOLS,
            "response_contract": {
                "type": "json_object",
                "shape": {
                    "done": "bool",
                    "focus": "optional short string",
                    "calls": [{"tool": "string", "args": "object"}]
                }
            }
        });

        let user_content = serde_json::to_string_pretty(&state_payload)
            .map_err(|e| IndexerError::Mcp(format!("failed to build agent prompt payload: {}", e)))?;

        let completion = client
            .complete(&[
                AgentMessage::system(system_prompt()),
                AgentMessage::user(user_content),
            ])
            .await?;

        finish_reason = completion.finish_reason.clone();
        usage_acc.add(completion.usage.as_ref());

        let command = parse_agent_step_command(&completion.content)?;
        let mut trace_calls = Vec::new();

        for call in command.calls.iter() {
            let mut args = normalize_args(&call.args);
            fill_missing_required_args(&call.tool, &mut args, &request);
            if !is_allowed_tool(&call.tool) {
                gaps.push(CoverageGap {
                    layer: "orchestration".to_string(),
                    reason: format!("agent requested unsupported tool '{}'", call.tool),
                    recommended_tool_call: None,
                });
                trace_calls.push(AgentTraceCall {
                    tool: call.tool.clone(),
                    args: args.clone(),
                    status: "rejected".to_string(),
                    error: Some("unsupported tool".to_string()),
                    });
                continue;
            }

            remember_suggested_call(
                &mut suggested_tool_calls,
                &mut suggested_keys,
                &call.tool,
                &args,
                "Agent-requested next data collection step",
            );

            match execute_tool(&call.tool, &args) {
                Ok(result) => {
                    apply_tool_result(&mut task_context, &call.tool, &args, &result);
                    mark_observed_layer(&mut observed, &call.tool);
                    trace_calls.push(AgentTraceCall {
                        tool: call.tool.clone(),
                        args,
                        status: "ok".to_string(),
                        error: None,
                    });
                }
                Err(err) => {
                    let layer = layer_for_tool(&call.tool).to_string();
                    gaps.push(CoverageGap {
                        layer: layer.clone(),
                        reason: format!("tool '{}' failed: {}", call.tool, err),
                        recommended_tool_call: Some(SuggestedToolCall {
                            tool: call.tool.clone(),
                            args: serde_json::json!({}),
                            reason: "Retry failed collection step".to_string(),
                        }),
                    });
                    trace_calls.push(AgentTraceCall {
                        tool: call.tool.clone(),
                        args,
                        status: "error".to_string(),
                        error: Some(err),
                    });
                }
            }
        }

        coverage = compute_coverage(&task_context, &observed);

        if request.include_trace {
            trace.push(AgentTraceStep {
                step,
                focus: command.focus.clone(),
                calls: trace_calls,
                note: if command.done && !coverage.complete {
                    Some(
                        "agent reported done before required coverage was complete".to_string(),
                    )
                } else {
                    None
                },
            });
        }

        if command.done && required_coverage_is_complete(&coverage) {
            break;
        }
    }

    if !timeout_reached && steps_taken >= max_steps && !required_coverage_is_complete(&coverage) {
        max_steps_reached = true;
    }

    coverage = compute_coverage(&task_context, &observed);

    let limit_reason = if timeout_reached {
        Some(format!(
            "collection stopped by timeout after {} ms",
            timeout_ms
        ))
    } else if max_steps_reached {
        Some(format!(
            "collection stopped after reaching max steps ({})",
            max_steps
        ))
    } else {
        None
    };

    let mut next_actions = Vec::new();
    for layer in missing_layers(&coverage) {
        let call = recommended_call_for_layer(layer, &request);
        gaps.push(CoverageGap {
            layer: layer.to_string(),
            reason: limit_reason
                .clone()
                .unwrap_or_else(|| "required layer is incomplete".to_string()),
            recommended_tool_call: Some(call.clone()),
        });
        next_actions.push(
            NextAction::new(call.tool.clone(), call.args.clone())
                .with_hint(call.reason.clone()),
        );
        remember_suggested_call(
            &mut suggested_tool_calls,
            &mut suggested_keys,
            &call.tool,
            &call.args,
            &call.reason,
        );
    }

    dedup_task_context(&mut task_context);

    let collection_meta = AgentCollectionMeta {
        provider: request.provider,
        model: request.model,
        endpoint: request.endpoint,
        steps_taken,
        elapsed_ms: started.elapsed().as_millis() as u64,
        timeout_reached,
        max_steps_reached,
        finish_reason,
        usage: usage_acc.into_usage(),
        trace,
    };

    Ok(AgentCollectionResult {
        task_context,
        coverage,
        gaps,
        collection_meta,
        next_actions,
        suggested_tool_calls,
    })
}

fn system_prompt() -> &'static str {
    "You are a context collection orchestrator for a code indexer.\n\
Return strictly one JSON object only (no markdown) with shape:\n\
{\"done\": bool, \"focus\": \"optional\", \"calls\": [{\"tool\": \"...\", \"args\": {...}}]}\n\
Rules:\n\
- Use only allowed tools from the input.\n\
- Prefer filling these required layers first: module_graph, file_import_graph, symbol_interaction_graph.\n\
- Defer optional layers if limits are tight.\n\
- Keep calls concise and deterministic."
}

fn parse_agent_step_command(content: &str) -> Result<AgentStepCommand> {
    if let Ok(cmd) = serde_json::from_str::<AgentStepCommand>(content) {
        return Ok(cmd);
    }

    let extracted = extract_json_object(content).ok_or_else(|| {
        IndexerError::Mcp(format!(
            "agent response is not valid JSON command: {}",
            summarize_for_error(content)
        ))
    })?;

    serde_json::from_str::<AgentStepCommand>(extracted).map_err(|e| {
        IndexerError::Mcp(format!(
            "agent command JSON parse error: {} (payload={})",
            e,
            summarize_for_error(content)
        ))
    })
}

fn extract_json_object(content: &str) -> Option<&str> {
    let start = content.find('{')?;
    let end = content.rfind('}')?;
    if end <= start {
        return None;
    }
    content.get(start..=end)
}

fn summarize_for_error(content: &str) -> String {
    const LIMIT: usize = 240;
    let trimmed = content.trim();
    if trimmed.len() <= LIMIT {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..LIMIT])
    }
}

fn normalize_args(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(_) => value.clone(),
        serde_json::Value::Null => serde_json::json!({}),
        _ => serde_json::json!({ "value": value.clone() }),
    }
}

fn fill_missing_required_args(
    tool: &str,
    args: &mut serde_json::Value,
    request: &AgentCollectionRequest,
) {
    let Some(map) = args.as_object_mut() else {
        return;
    };

    let inferred_symbol = infer_symbol_name_from_query(&request.query);

    match tool {
        "find_references" => {
            if !map.contains_key("name") {
                if let Some(symbol) = inferred_symbol {
                    map.insert("name".to_string(), serde_json::Value::String(symbol));
                }
            }
            map.entry("include_callers".to_string())
                .or_insert(serde_json::Value::Bool(true));
            map.entry("depth".to_string())
                .or_insert(serde_json::Value::from(2_u64));
        }
        "analyze_call_graph" => {
            if !map.contains_key("function") {
                if let Some(symbol) = inferred_symbol {
                    map.insert("function".to_string(), serde_json::Value::String(symbol));
                }
            }
            map.entry("direction".to_string())
                .or_insert(serde_json::Value::String("both".to_string()));
            map.entry("depth".to_string())
                .or_insert(serde_json::Value::from(2_u64));
        }
        "get_file_outline" | "get_imports" => {
            if !map.contains_key("file") {
                if let Some(file) = request.file.as_ref() {
                    map.insert("file".to_string(), serde_json::Value::String(file.clone()));
                }
            }
        }
        "find_module_dependencies" => {
            if !map.contains_key("module_name") {
                if let Some(module) = infer_module_name_from_file(request.file.as_deref()) {
                    map.insert("module_name".to_string(), serde_json::Value::String(module));
                }
            }
            map.entry("workspace_path".to_string())
                .or_insert(serde_json::Value::String(".".to_string()));
        }
        "list_modules" => {
            map.entry("workspace_path".to_string())
                .or_insert(serde_json::Value::String(".".to_string()));
        }
        "search_symbols" => {
            if !map.contains_key("query") {
                map.insert(
                    "query".to_string(),
                    serde_json::Value::String(request.query.clone()),
                );
            }
            if !map.contains_key("file") {
                if let Some(file) = request.file.as_ref() {
                    map.insert("file".to_string(), serde_json::Value::String(file.clone()));
                }
            }
        }
        _ => {}
    }
}

fn infer_symbol_name_from_query(query: &str) -> Option<String> {
    let mut candidates: Vec<String> = query
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|t| t.len() >= 3)
        .map(|s| s.to_string())
        .collect();
    if candidates.is_empty() {
        return None;
    }

    // Prefer identifier-like symbols over natural language words.
    candidates.sort_by_key(|token| {
        let has_underscore = token.contains('_');
        let has_uppercase = token.chars().any(|c| c.is_ascii_uppercase());
        let score = (has_underscore as i32) * 2 + (has_uppercase as i32);
        (-score, -(token.len() as i32))
    });

    candidates.into_iter().next()
}

fn infer_module_name_from_file(file: Option<&str>) -> Option<String> {
    let file = file?;
    let normalized = file.replace('\\', "/");
    let trimmed = normalized.strip_prefix("./").unwrap_or(&normalized);
    if let Some(rest) = trimmed.strip_prefix("src/") {
        let module = rest.split('/').next()?;
        if !module.is_empty() {
            return Some(module.to_string());
        }
    }
    trimmed
        .split('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn is_allowed_tool(tool: &str) -> bool {
    ALLOWED_TOOLS.contains(&tool)
}

fn layer_for_tool(tool: &str) -> &'static str {
    match tool {
        "list_modules" | "find_module_dependencies" => "module_graph",
        "get_imports" => "file_import_graph",
        "search_symbols" | "find_references" | "analyze_call_graph" | "get_file_outline" => {
            "symbol_interaction_graph"
        }
        "list_dependencies" | "get_dependency_info" | "get_dependency_source" => {
            "deps_touchpoints"
        }
        "get_architecture_summary"
        | "get_stats"
        | "get_doc_section"
        | "get_project_compass"
        | "get_project_commands" => "docs_config_digest",
        _ => "orchestration",
    }
}

fn apply_tool_result(
    digest: &mut TaskContextDigest,
    tool: &str,
    args: &serde_json::Value,
    result: &serde_json::Value,
) {
    match tool {
        "list_modules" => {
            if let Some(modules) = result.get("modules").and_then(|v| v.as_array()) {
                for module in modules {
                    let name = module
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    if name.is_empty() {
                        continue;
                    }
                    if let Some(deps) = module
                        .get("internal_dependencies")
                        .and_then(|v| v.as_array())
                    {
                        for dep in deps.iter().filter_map(|v| v.as_str()) {
                            digest.module_graph.push(ModuleDependencyEdge {
                                from: name.to_string(),
                                to: dep.to_string(),
                                relation: "depends_on".to_string(),
                            });
                        }
                    }
                }
            }
        }
        "find_module_dependencies" => {
            let module = result
                .get("module")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if !module.is_empty() {
                if let Some(deps) = result.get("depends_on").and_then(|v| v.as_array()) {
                    for dep in deps.iter().filter_map(|v| v.as_str()) {
                        digest.module_graph.push(ModuleDependencyEdge {
                            from: module.to_string(),
                            to: dep.to_string(),
                            relation: "depends_on".to_string(),
                        });
                    }
                }
                if let Some(depended_by) = result.get("depended_by").and_then(|v| v.as_array()) {
                    for source in depended_by.iter().filter_map(|v| v.as_str()) {
                        digest.module_graph.push(ModuleDependencyEdge {
                            from: source.to_string(),
                            to: module.to_string(),
                            relation: "depends_on".to_string(),
                        });
                    }
                }
            }
        }
        "get_imports" => {
            if let Some(imports) = result.get("imports").and_then(|v| v.as_array()) {
                for import in imports {
                    let from_file = import
                        .get("file_path")
                        .or_else(|| import.get("from_file"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let imported_path = import
                        .get("imported_path")
                        .or_else(|| import.get("path"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    if from_file.is_empty() || imported_path.is_empty() {
                        continue;
                    }
                    digest.file_import_graph.push(FileImportEdge {
                        from_file: from_file.to_string(),
                        imported_path: imported_path.to_string(),
                        imported_symbol: import
                            .get("imported_symbol")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    });
                }
            }
        }
        "search_symbols" => {
            if let Some(results) = result.as_array() {
                let query_anchor = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("query");
                for item in results {
                    let symbol = item.get("symbol").unwrap_or(item);
                    if let Some(name) = symbol.get("name").and_then(|v| v.as_str()) {
                        digest.symbol_interactions.push(SymbolInteractionEdge {
                            from: format!("query:{}", query_anchor),
                            to: name.to_string(),
                            relation: "matches".to_string(),
                            location: symbol
                                .get("location")
                                .and_then(|loc| loc.get("file_path"))
                                .and_then(|v| v.as_str())
                                .map(|file| file.to_string()),
                        });
                    }
                }
            }
        }
        "find_references" => {
            if let Some(references) = result.get("references").and_then(|v| v.as_array()) {
                for r in references {
                    let symbol = r
                        .get("symbol_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let file = r
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let line = r.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
                    if symbol.is_empty() || file.is_empty() {
                        continue;
                    }
                    digest.symbol_interactions.push(SymbolInteractionEdge {
                        from: format!("{}:{}", file, line),
                        to: symbol.to_string(),
                        relation: "references".to_string(),
                        location: Some(format!("{}:{}", file, line)),
                    });
                }
            }

            if let Some(callers) = result.get("callers").and_then(|v| v.as_array()) {
                for c in callers {
                    let from = c
                        .get("symbol_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let target = args.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                    if from.is_empty() || target.is_empty() {
                        continue;
                    }
                    let loc = c
                        .get("file_path")
                        .and_then(|f| f.as_str())
                        .zip(c.get("line").and_then(|l| l.as_u64()))
                        .map(|(f, l)| format!("{}:{}", f, l));
                    digest.symbol_interactions.push(SymbolInteractionEdge {
                        from: from.to_string(),
                        to: target.to_string(),
                        relation: "calls".to_string(),
                        location: loc,
                    });
                }
            }
        }
        "analyze_call_graph" => {
            if let Some(call_graph) = result.get("call_graph") {
                if let Some(edges) = call_graph.get("edges").and_then(|v| v.as_array()) {
                    for e in edges {
                        let from = e.get("from").and_then(|v| v.as_str()).unwrap_or_default();
                        let to = e
                            .get("to")
                            .and_then(|v| v.as_str())
                            .or_else(|| e.get("callee_name").and_then(|v| v.as_str()))
                            .unwrap_or_default();
                        if from.is_empty() || to.is_empty() {
                            continue;
                        }
                        let loc = e
                            .get("call_site_file")
                            .and_then(|f| f.as_str())
                            .zip(e.get("call_site_line").and_then(|l| l.as_u64()))
                            .map(|(f, l)| format!("{}:{}", f, l));
                        digest.symbol_interactions.push(SymbolInteractionEdge {
                            from: from.to_string(),
                            to: to.to_string(),
                            relation: "calls".to_string(),
                            location: loc,
                        });
                    }
                }
            }
        }
        "get_file_outline" => {
            if let Some(file_meta) = result.get("file_meta") {
                if let Some(doc1) = file_meta.get("doc1").and_then(|v| v.as_str()) {
                    digest.docs_config_digest.push(DocConfigDigestEntry {
                        source: "file_meta.doc1".to_string(),
                        summary: doc1.to_string(),
                    });
                }
                if let Some(purpose) = file_meta.get("purpose").and_then(|v| v.as_str()) {
                    digest.docs_config_digest.push(DocConfigDigestEntry {
                        source: "file_meta.purpose".to_string(),
                        summary: purpose.to_string(),
                    });
                }
            }
        }
        "get_architecture_summary" => {
            digest.docs_config_digest.push(DocConfigDigestEntry {
                source: "architecture_summary".to_string(),
                summary: summarize_json(result, 280),
            });
        }
        "get_stats" => {
            if result.get("workspace").is_some() || result.get("architecture").is_some() {
                digest.docs_config_digest.push(DocConfigDigestEntry {
                    source: "stats.extended".to_string(),
                    summary: summarize_json(result, 280),
                });
            }
        }
        "list_dependencies" => {
            if let Some(deps) = result.get("dependencies").and_then(|v| v.as_array()) {
                for dep in deps {
                    if let Some(name) = dep.get("name").and_then(|v| v.as_str()) {
                        let version = dep
                            .get("version")
                            .and_then(|v| v.as_str())
                            .map(|v| format!("version={}", v));
                        digest.deps_touchpoints.push(DependencyTouchpoint {
                            dependency: name.to_string(),
                            symbol: None,
                            detail: version,
                        });
                    }
                }
            }
        }
        "get_dependency_info" => {
            if let Some(name) = result.get("name").and_then(|v| v.as_str()) {
                digest.deps_touchpoints.push(DependencyTouchpoint {
                    dependency: name.to_string(),
                    symbol: None,
                    detail: result
                        .get("version")
                        .and_then(|v| v.as_str())
                        .map(|v| format!("version={}", v)),
                });
            }
        }
        "get_dependency_source" => {
            let symbol = args
                .get("symbol_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let dep = args
                .get("dependency")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown-dependency");
            digest.deps_touchpoints.push(DependencyTouchpoint {
                dependency: dep.to_string(),
                symbol,
                detail: Some("dependency source context resolved".to_string()),
            });
        }
        "get_doc_section" | "get_project_compass" | "get_project_commands" => {
            digest.docs_config_digest.push(DocConfigDigestEntry {
                source: tool.to_string(),
                summary: summarize_json(result, 280),
            });
        }
        _ => {}
    }
}

fn summarize_json(value: &serde_json::Value, limit: usize) -> String {
    let text = serde_json::to_string(value).unwrap_or_default();
    if text.len() <= limit {
        text
    } else {
        format!("{}...", &text[..limit])
    }
}

fn compute_coverage(task_context: &TaskContextDigest, observed: &LayerObserved) -> ContextCoverage {
    let module_graph = observed.module_graph || !task_context.module_graph.is_empty();
    let file_import_graph = observed.file_import_graph || !task_context.file_import_graph.is_empty();
    let symbol_interaction_graph =
        observed.symbol_interaction_graph || !task_context.symbol_interactions.is_empty();
    let deps_touchpoints = observed.deps_touchpoints || !task_context.deps_touchpoints.is_empty();
    let docs_config_digest =
        observed.docs_config_digest || !task_context.docs_config_digest.is_empty();
    let complete = module_graph && file_import_graph && symbol_interaction_graph;

    ContextCoverage {
        module_graph,
        file_import_graph,
        symbol_interaction_graph,
        deps_touchpoints,
        docs_config_digest,
        complete,
    }
}

#[derive(Debug, Default, Clone)]
struct LayerObserved {
    module_graph: bool,
    file_import_graph: bool,
    symbol_interaction_graph: bool,
    deps_touchpoints: bool,
    docs_config_digest: bool,
}

fn mark_observed_layer(observed: &mut LayerObserved, tool: &str) {
    match tool {
        "list_modules" | "find_module_dependencies" => observed.module_graph = true,
        "get_imports" => observed.file_import_graph = true,
        "search_symbols" | "find_references" | "analyze_call_graph" | "get_file_outline" => {
            observed.symbol_interaction_graph = true;
        }
        "list_dependencies" | "get_dependency_info" | "get_dependency_source" => {
            observed.deps_touchpoints = true;
        }
        "get_architecture_summary"
        | "get_stats"
        | "get_doc_section"
        | "get_project_compass"
        | "get_project_commands" => observed.docs_config_digest = true,
        _ => {}
    }
}

fn required_coverage_is_complete(coverage: &ContextCoverage) -> bool {
    coverage.module_graph && coverage.file_import_graph && coverage.symbol_interaction_graph
}

fn missing_layers(coverage: &ContextCoverage) -> Vec<&'static str> {
    let mut layers = Vec::new();
    if !coverage.module_graph {
        layers.push("module_graph");
    }
    if !coverage.file_import_graph {
        layers.push("file_import_graph");
    }
    if !coverage.symbol_interaction_graph {
        layers.push("symbol_interaction_graph");
    }
    layers
}

fn recommended_call_for_layer(layer: &str, request: &AgentCollectionRequest) -> SuggestedToolCall {
    match layer {
        "module_graph" => SuggestedToolCall {
            tool: "list_modules".to_string(),
            args: serde_json::json!({
                "workspace_path": "."
            }),
            reason: "Collect module dependency layer".to_string(),
        },
        "file_import_graph" => SuggestedToolCall {
            tool: "get_imports".to_string(),
            args: serde_json::json!({
                "file": request.file.clone().unwrap_or_else(|| ".".to_string()),
                "resolve": true
            }),
            reason: "Collect file-level import interactions".to_string(),
        },
        "symbol_interaction_graph" => SuggestedToolCall {
            tool: "find_references".to_string(),
            args: serde_json::json!({
                "name": request.query,
                "include_callers": true,
                "depth": 2,
                "limit": 50
            }),
            reason: "Collect symbol-level references/callers graph".to_string(),
        },
        _ => SuggestedToolCall {
            tool: "search_symbols".to_string(),
            args: serde_json::json!({
                "query": request.query,
                "limit": 20
            }),
            reason: "Continue context expansion".to_string(),
        },
    }
}

fn dedup_task_context(task_context: &mut TaskContextDigest) {
    dedup_by_key(
        &mut task_context.module_graph,
        |edge| format!("{}|{}|{}", edge.from, edge.to, edge.relation),
    );
    dedup_by_key(
        &mut task_context.file_import_graph,
        |edge| format!(
            "{}|{}|{}",
            edge.from_file,
            edge.imported_path,
            edge.imported_symbol.clone().unwrap_or_default()
        ),
    );
    dedup_by_key(
        &mut task_context.symbol_interactions,
        |edge| {
            format!(
                "{}|{}|{}|{}",
                edge.from,
                edge.to,
                edge.relation,
                edge.location.clone().unwrap_or_default()
            )
        },
    );
    dedup_by_key(
        &mut task_context.deps_touchpoints,
        |tp| {
            format!(
                "{}|{}|{}",
                tp.dependency,
                tp.symbol.clone().unwrap_or_default(),
                tp.detail.clone().unwrap_or_default()
            )
        },
    );
    dedup_by_key(
        &mut task_context.docs_config_digest,
        |item| format!("{}|{}", item.source, item.summary),
    );
}

fn dedup_by_key<T, F>(items: &mut Vec<T>, key: F)
where
    F: Fn(&T) -> String,
{
    let mut seen: HashSet<String> = HashSet::new();
    items.retain(|item| seen.insert(key(item)));
}

fn remember_suggested_call(
    suggested_tool_calls: &mut Vec<SuggestedToolCall>,
    keys: &mut HashSet<String>,
    tool: &str,
    args: &serde_json::Value,
    reason: &str,
) {
    let key = format!(
        "{}:{}",
        tool,
        serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string())
    );
    if keys.insert(key) {
        suggested_tool_calls.push(SuggestedToolCall {
            tool: tool.to_string(),
            args: args.clone(),
            reason: reason.to_string(),
        });
    }
}

#[derive(Debug, Default)]
struct UsageAccumulator {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    has_values: bool,
}

impl UsageAccumulator {
    fn add(&mut self, usage: Option<&AgentUsage>) {
        if let Some(usage) = usage {
            self.prompt_tokens += usage.prompt_tokens;
            self.completion_tokens += usage.completion_tokens;
            self.total_tokens += usage.total_tokens;
            self.has_values = true;
        }
    }

    fn into_usage(self) -> Option<AgentTokenUsage> {
        if self.has_values {
            Some(AgentTokenUsage {
                prompt_tokens: self.prompt_tokens,
                completion_tokens: self.completion_tokens,
                total_tokens: self.total_tokens,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::agent_client::AgentClientConfig;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn completion_body(content: &str) -> String {
        serde_json::json!({
            "choices": [{
                "message": { "content": content },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        })
        .to_string()
    }

    async fn spawn_mock_agent_server(bodies: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server");
        let addr = listener.local_addr().expect("local addr");

        tokio::spawn(async move {
            for body in bodies {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let mut buf = vec![0_u8; 8192];
                let _ = socket.read(&mut buf).await;

                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });

        format!("http://{}/v1", addr)
    }

    #[tokio::test]
    async fn orchestrator_collects_required_layers_over_multiple_steps() {
        let endpoint = spawn_mock_agent_server(vec![
            completion_body(
                r#"{"done":false,"focus":"modules+imports","calls":[{"tool":"list_modules","args":{"workspace_path":"."}},{"tool":"get_imports","args":{"file":"src/app.rs","resolve":true}}]}"#,
            ),
            completion_body(
                r#"{"done":true,"focus":"interactions","calls":[{"tool":"find_references","args":{"name":"run","include_callers":true,"depth":2}}]}"#,
            ),
        ])
        .await;

        let client = AgentClient::new(AgentClientConfig {
            provider: "local".to_string(),
            model: "gpt-5.2".to_string(),
            endpoint,
            api_key: Some("test-token".to_string()),
            timeout: std::time::Duration::from_secs(5),
        })
        .expect("agent client");

        let result = run_agent_context_collection(
            &client,
            AgentCollectionRequest {
                query: "trace run flow".to_string(),
                file: Some("src/app.rs".to_string()),
                task_hint: Some("debugging".to_string()),
                timeout_ms: Some(15_000),
                max_steps: Some(6),
                include_trace: true,
                provider: "local".to_string(),
                model: Some("gpt-5.2".to_string()),
                endpoint: Some("http://mock.local/v1".to_string()),
            },
            |tool, _args| match tool {
                "list_modules" => Ok(serde_json::json!({
                    "modules": [
                        {"name": "app", "internal_dependencies": ["core"]}
                    ]
                })),
                "get_imports" => Ok(serde_json::json!({
                    "imports": [
                        {
                            "file_path": "src/app.rs",
                            "imported_path": "crate::core",
                            "imported_symbol": "run"
                        }
                    ]
                })),
                "find_references" => Ok(serde_json::json!({
                    "references": [
                        {
                            "symbol_name": "run",
                            "file_path": "src/app.rs",
                            "line": 10
                        }
                    ],
                    "callers": [
                        {
                            "symbol_name": "main",
                            "file_path": "src/main.rs",
                            "line": 4
                        }
                    ]
                })),
                _ => Err(format!("unexpected tool: {}", tool)),
            },
        )
        .await
        .expect("orchestrator result");

        assert!(result.coverage.complete);
        assert!(result.coverage.module_graph);
        assert!(result.coverage.file_import_graph);
        assert!(result.coverage.symbol_interaction_graph);
        assert!(result.gaps.is_empty());
        assert!(!result.task_context.module_graph.is_empty());
        assert!(!result.task_context.file_import_graph.is_empty());
        assert!(!result.task_context.symbol_interactions.is_empty());
        assert_eq!(result.collection_meta.steps_taken, 2);
        assert_eq!(result.collection_meta.trace.len(), 2);
    }

    #[tokio::test]
    async fn orchestrator_reports_partial_with_gaps_on_step_limit() {
        let endpoint = spawn_mock_agent_server(vec![completion_body(
            r#"{"done":false,"focus":"modules","calls":[{"tool":"list_modules","args":{"workspace_path":"."}}]}"#,
        )])
        .await;

        let client = AgentClient::new(AgentClientConfig {
            provider: "local".to_string(),
            model: "gpt-5.2".to_string(),
            endpoint,
            api_key: Some("test-token".to_string()),
            timeout: std::time::Duration::from_secs(5),
        })
        .expect("agent client");

        let result = run_agent_context_collection(
            &client,
            AgentCollectionRequest {
                query: "collect context".to_string(),
                file: Some("src/lib.rs".to_string()),
                task_hint: None,
                timeout_ms: Some(5_000),
                max_steps: Some(1),
                include_trace: false,
                provider: "local".to_string(),
                model: Some("gpt-5.2".to_string()),
                endpoint: Some("http://mock.local/v1".to_string()),
            },
            |_tool, _args| Ok(serde_json::json!({ "modules": [] })),
        )
        .await
        .expect("orchestrator result");

        assert!(!result.coverage.complete);
        assert!(result.collection_meta.max_steps_reached);
        assert!(!result.gaps.is_empty());
        assert!(!result.next_actions.is_empty());
    }

    #[tokio::test]
    async fn orchestrator_fails_on_invalid_agent_json_command() {
        let endpoint = spawn_mock_agent_server(vec![completion_body("not-json-command")]).await;

        let client = AgentClient::new(AgentClientConfig {
            provider: "local".to_string(),
            model: "gpt-5.2".to_string(),
            endpoint,
            api_key: Some("test-token".to_string()),
            timeout: std::time::Duration::from_secs(5),
        })
        .expect("agent client");

        let err = run_agent_context_collection(
            &client,
            AgentCollectionRequest {
                query: "collect context".to_string(),
                file: None,
                task_hint: None,
                timeout_ms: Some(5_000),
                max_steps: Some(1),
                include_trace: false,
                provider: "local".to_string(),
                model: Some("gpt-5.2".to_string()),
                endpoint: Some("http://mock.local/v1".to_string()),
            },
            |_tool, _args| Ok(serde_json::json!({})),
        )
        .await
        .expect_err("invalid json must fail");

        assert!(err.to_string().contains("agent response is not valid JSON command"));
    }

    #[test]
    fn fill_missing_required_args_uses_request_fallbacks() {
        let request = AgentCollectionRequest {
            query: "trace prepare_context_with_agent callers".to_string(),
            file: Some("src/mcp/server.rs".to_string()),
            task_hint: Some("understanding".to_string()),
            timeout_ms: Some(1000),
            max_steps: Some(1),
            include_trace: false,
            provider: "local".to_string(),
            model: Some("gpt-5.2".to_string()),
            endpoint: Some("http://127.0.0.1:8317/v1".to_string()),
        };

        let mut refs_args = serde_json::json!({});
        fill_missing_required_args("find_references", &mut refs_args, &request);
        assert_eq!(
            refs_args.get("name").and_then(|v| v.as_str()),
            Some("prepare_context_with_agent")
        );

        let mut graph_args = serde_json::json!({});
        fill_missing_required_args("analyze_call_graph", &mut graph_args, &request);
        assert_eq!(
            graph_args.get("function").and_then(|v| v.as_str()),
            Some("prepare_context_with_agent")
        );

        let mut imports_args = serde_json::json!({});
        fill_missing_required_args("get_imports", &mut imports_args, &request);
        assert_eq!(
            imports_args.get("file").and_then(|v| v.as_str()),
            Some("src/mcp/server.rs")
        );
    }
}
