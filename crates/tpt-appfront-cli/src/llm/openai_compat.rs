//! OpenAI-compatible backend for `generate --llm`.
//!
//! Covers every provider that exposes an OpenAI-style `chat/completions`
//! endpoint (OpenAI, OpenRouter, Grok, and Ollama's `/v1/chat/completions`) via a
//! single request/response shape. The per-provider differences (base URL, API-key
//! env var, default model) live in the `PROVIDER_CONFIGS` table in `mod.rs`, so
//! adding such a provider is a one-line table change — no new code here.
//!
//! Key-less providers (Ollama) pass `None` for the API key; `build_provider`
//! validates the key requirement before constructing this provider.

use anyhow::{bail, Context};

use crate::llm::LlmProvider;

/// Request body for an OpenAI-compatible `chat/completions` endpoint.
#[derive(serde::Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(serde::Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

/// One message in an OpenAI-compatible `chat/completions` response.
#[derive(serde::Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

/// Top-level OpenAI-compatible `chat/completions` response.
#[derive(serde::Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(serde::Deserialize)]
struct Choice {
    message: ResponseMessage,
}

pub struct OpenAiCompatProvider {
    provider: String,
    api_key: Option<String>,
    model: String,
    endpoint: String,
}

impl OpenAiCompatProvider {
    /// Builds the provider. `endpoint` is the config-table default or a
    /// `--base-url` override; `api_key` is `None` for key-less providers.
    pub fn new(
        provider: &str,
        api_key: Option<String>,
        model: String,
        endpoint: &str,
    ) -> Self {
        Self {
            provider: provider.to_string(),
            api_key,
            model,
            endpoint: endpoint.to_string(),
        }
    }
}

impl LlmProvider for OpenAiCompatProvider {
    fn name(&self) -> &str {
        &self.provider
    }

    fn complete(&self, system: &str, user: &str) -> anyhow::Result<String> {
        let req = ChatRequest {
            model: &self.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: system,
                },
                ChatMessage {
                    role: "user",
                    content: user,
                },
            ],
            max_tokens: Some(2048),
        };

        let mut builder = ureq::post(&self.endpoint)
            .set("content-type", "application/json")
            .set("accept", "application/json");
        if let Some(key) = &self.api_key {
            // Ollama (and some self-hosted OpenAI-compatible servers) ignore the
            // auth header; sending `Bearer` is harmless when a key is present.
            builder = builder.set("authorization", &format!("Bearer {key}"));
        }

        let resp = builder.send_json(&req).with_context(|| {
            format!(
                "request to the `{}` chat/completions endpoint failed (network or 4xx/5xx)",
                self.provider
            )
        })?;

        if resp.status() != 200 {
            let status = resp.status();
            let body = resp
                .into_string()
                .unwrap_or_else(|_| "<no body>".to_string());
            bail!("`{}` API returned {status}: {body}", self.provider);
        }

        let parsed: ChatResponse = resp
            .into_json()
            .with_context(|| format!("failed to parse the `{}` API JSON response", self.provider))?;

        let text = parsed
            .choices
            .into_iter()
            .filter_map(|c| c.message.content)
            .collect::<Vec<_>>()
            .join("\n");

        if text.trim().is_empty() {
            bail!("`{}` API returned an empty completion", self.provider);
        }
        Ok(text)
    }
}
