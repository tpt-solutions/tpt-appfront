//! Anthropic (`claude`) backend for `generate --llm`.
//!
//! Implements [`crate::llm::LlmProvider`] over the Anthropic Messages API using
//! `ureq` (blocking, dependency-light) + `serde_json`. The model id comes from
//! `--model` (defaulted by the config table); `endpoint` is the table default or
//! a `--base-url` override. `ANTHROPIC_API_KEY` must be set in the environment;
//! [`crate::llm::build_provider`] returns a clear error when it isn't, so `--llm`
//! never silently falls back to the offline generator.

use anyhow::{bail, Context};

use crate::llm::LlmProvider;

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Request body for the Anthropic Messages API.
#[derive(serde::Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
}

#[derive(serde::Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

/// One content block in an Anthropic Messages response.
#[derive(serde::Deserialize)]
struct ResponseBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

/// Top-level Anthropic Messages response.
#[derive(serde::Deserialize)]
struct MessagesResponse {
    content: Vec<ResponseBlock>,
}

pub struct AnthropicProvider {
    api_key: String,
    model: String,
    endpoint: String,
}

impl AnthropicProvider {
    /// Builds the provider. `endpoint` is the config-table default or a
    /// `--base-url` override.
    pub fn new(api_key: String, model: String, endpoint: &str) -> Self {
        Self {
            api_key,
            model,
            endpoint: endpoint.to_string(),
        }
    }
}

impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn complete(&self, system: &str, user: &str) -> anyhow::Result<String> {
        let req = MessagesRequest {
            model: &self.model,
            max_tokens: 2048,
            system,
            messages: vec![Message {
                role: "user",
                content: user,
            }],
        };

        let resp = ureq::post(&self.endpoint)
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", ANTHROPIC_VERSION)
            .set("content-type", "application/json")
            .send_json(&req)
            .with_context(|| {
                "request to the Anthropic Messages API failed (network or 4xx/5xx)"
            })?;

        if resp.status() != 200 {
            let status = resp.status();
            let body = resp
                .into_string()
                .unwrap_or_else(|_| "<no body>".to_string());
            bail!("Anthropic API returned {status}: {body}");
        }

        let parsed: MessagesResponse = resp
            .into_json()
            .with_context(|| "failed to parse the Anthropic API JSON response")?;

        let text = parsed
            .content
            .into_iter()
            .filter(|b| b.kind == "text")
            .filter_map(|b| b.text)
            .collect::<Vec<_>>()
            .join("\n");

        if text.trim().is_empty() {
            bail!("Anthropic API returned an empty completion");
        }
        Ok(text)
    }
}
