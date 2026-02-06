use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{IndexerError, Result};

/// Runtime configuration for OpenAI-compatible agent endpoint.
#[derive(Debug, Clone)]
pub struct AgentClientConfig {
    pub provider: String,
    pub model: String,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
}

/// Minimal chat message for chat-completions payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: String,
    pub content: String,
}

impl AgentMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }
}

/// Token usage returned by chat-completions endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentUsage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

/// Parsed response from one agent completion call.
#[derive(Debug, Clone)]
pub struct AgentCompletion {
    pub content: String,
    pub finish_reason: Option<String>,
    pub usage: Option<AgentUsage>,
}

pub struct AgentClient {
    config: AgentClientConfig,
    http: reqwest::Client,
}

impl AgentClient {
    pub fn new(config: AgentClientConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| IndexerError::Mcp(format!("failed to build agent http client: {}", e)))?;

        Ok(Self { config, http })
    }

    pub async fn complete(&self, messages: &[AgentMessage]) -> Result<AgentCompletion> {
        if messages.is_empty() {
            return Err(IndexerError::Mcp(
                "agent completion requires at least one message".to_string(),
            ));
        }

        let url = self.chat_completions_url();
        let payload = ChatCompletionsRequest {
            model: self.config.model.clone(),
            messages: messages.to_vec(),
            temperature: Some(0.0),
            stream: Some(false),
        };

        let mut request = self.http.post(&url).json(&payload);
        if let Some(api_key) = self.config.api_key.as_ref() {
            request = request.bearer_auth(api_key);
        }
        let response = request
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    IndexerError::Mcp(format!(
                        "agent request timed out after {:?} (provider={}, model={})",
                        self.config.timeout, self.config.provider, self.config.model
                    ))
                } else {
                    IndexerError::Mcp(format!(
                        "agent request failed (provider={}, model={}): {}",
                        self.config.provider, self.config.model, e
                    ))
                }
            })?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| IndexerError::Mcp(format!("failed to read agent response body: {}", e)))?;

        if !status.is_success() {
            return Err(IndexerError::Mcp(format!(
                "agent endpoint returned HTTP {}: {}",
                status,
                truncate_for_error(&body)
            )));
        }

        let parsed: ChatCompletionsResponse = serde_json::from_str(&body).map_err(|e| {
            IndexerError::Mcp(format!(
                "invalid JSON from agent endpoint: {} (body={})",
                e,
                truncate_for_error(&body)
            ))
        })?;

        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| IndexerError::Mcp("agent response had no choices".to_string()))?;

        let content = choice
            .message
            .content
            .to_text()
            .ok_or_else(|| IndexerError::Mcp("agent response had empty message content".to_string()))?;

        Ok(AgentCompletion {
            content,
            finish_reason: choice.finish_reason,
            usage: parsed.usage,
        })
    }

    fn chat_completions_url(&self) -> String {
        let endpoint = self.config.endpoint.trim().trim_end_matches('/');
        if endpoint.ends_with("/chat/completions") {
            endpoint.to_string()
        } else if endpoint.ends_with("/v1") {
            format!("{}/chat/completions", endpoint)
        } else {
            format!("{}/v1/chat/completions", endpoint)
        }
    }
}

fn truncate_for_error(value: &str) -> String {
    const LIMIT: usize = 400;
    if value.len() <= LIMIT {
        value.to_string()
    } else {
        format!("{}...", &value[..LIMIT])
    }
}

#[derive(Debug, Clone, Serialize)]
struct ChatCompletionsRequest {
    model: String,
    messages: Vec<AgentMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatCompletionsResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<AgentUsage>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatMessageResponse {
    content: ChatContent,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ChatContent {
    Text(String),
    Parts(Vec<ChatContentPart>),
}

impl ChatContent {
    fn to_text(self) -> Option<String> {
        match self {
            ChatContent::Text(text) => {
                let trimmed = text.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }
            ChatContent::Parts(parts) => {
                let joined = parts
                    .into_iter()
                    .filter_map(|p| p.text)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if joined.is_empty() {
                    None
                } else {
                    Some(joined)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ChatContentPart {
    #[serde(default)]
    text: Option<String>,
}
