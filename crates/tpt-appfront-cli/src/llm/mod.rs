//! Live-LLM-backed `generate --llm` mode.
//!
//! Everything in this module is compiled only under the `llm` Cargo feature so
//! the default CLI install stays network-free. The offline, rule-based generator
//! in `crate::generate` is always available; `--llm` is an opt-in that calls a
//! model provider to produce a `view!` snippet for open-ended prompts.
//!
//! Design notes:
//! - `LlmProvider` is the single trait every backend implements. `anthropic.rs`
//!   covers the native Anthropic Messages API; `openai_compat.rs` covers any
//!   OpenAI-compatible `chat/completions` endpoint via a provider *config table*
//!   (`PROVIDER_CONFIGS`). Adding a provider is a table entry, not a new code path.
//! - No model ids are pinned. Model ids change too often for a default to stay
//!   correct, so when `--model` is omitted the model is chosen from the provider's
//!   live model list (see [`choose_model`]). `--list-models` shows that list.
//! - The system prompt is derived from `view!`'s supported tag set (mirrored in
//!   [`VIEW_TAGS`], see its doc comment for the proc-macro-crate constraint)
//!   plus few-shot examples reused from `crate::generate::PATTERNS`. Both are
//!   single sources of truth, not hand-duplicated prose that can drift.
//! - Response handling extracts the first fenced code block and syntax-checks it
//!   with `syn`. On a close-but-imperfect response it never hard-errors: it
//!   returns the snippet with a `// WARNING:` banner so the user can fix it by
//!   hand, rather than the tool failing the whole command.

mod anthropic;
mod openai_compat;

use anyhow::{bail, Context};

use crate::generate::PATTERNS;

/// The `view!` tags the system prompt advertises. This mirrors
/// `tpt-appfront-macros/src/view.rs`'s `TAGS` (the macro's grammar). It lives
/// here because `tpt-appfront-macros` is a `proc-macro` crate and cannot export
/// plain `const`s, and it must not depend on `tpt-appfront-core` (that would be
/// a dependency cycle). Keep the two lists in sync. `build_system_prompt`'s
/// test asserts every tag below appears in the prompt.
pub(crate) const VIEW_TAGS: &[&str] = &[
    "Container",
    "Heading",
    "Text",
    "Button",
    "Input",
    "Textarea",
    "Checkbox",
    "Select",
    "Radio",
    "List",
    "DataGrid",
    "Image",
    "Link",
    "Media",
];

/// A model backend that can complete a (system, user) prompt pair and return
/// free-form text (the LLM's reply, which may contain prose + a fenced code
/// block). Concrete providers live in submodules (`anthropic`, `openai_compat`).
pub trait LlmProvider {
    /// Human-readable provider id (e.g. `"anthropic"`).
    fn name(&self) -> &str;
    /// The model id requests are sent to, after resolution.
    fn model(&self) -> &str;
    /// Run the completion. Network/API errors surface as [`anyhow::Error`];
    /// the provider must give a clear message (e.g. a missing API key) rather
    /// than silently falling back to the offline generator.
    fn complete(&self, system: &str, user: &str) -> anyhow::Result<String>;
}

/// How a provider's model-list response is shaped. Only the shape differs between providers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelList {
    /// `{"data": [{"id": ..}]}`. Used by OpenAI-compatible servers and by Anthropic (which
    /// adds a `created_at` string instead of `created`).
    Data,
    /// A bare JSON array of `{"id": ..}`. Used by Together.
    Array,
    /// Google's native `{"models": [{"name": "models/..", "supportedGenerationMethods": [..]}]}`.
    Gemini,
    /// No list endpoint. Azure models are deployments, which the user names with `--model`.
    None,
}

/// Static configuration for each `--provider` choice. `openai_compat` providers
/// share one request/response shape. Only these fields differ between them.
pub(crate) struct ProviderConfig {
    /// Default endpoint for this provider's `chat/completions` (or Messages) API.
    /// Empty when the user must supply one (Azure).
    pub base_url: &'static str,
    /// Env var holding the API key, or `None` for key-less local providers.
    pub key_env: Option<&'static str>,
    /// Endpoint that lists the models the key can use, or `None` when there is none.
    pub models_url: Option<&'static str>,
    /// Shape of that endpoint's response.
    pub list: ModelList,
}

/// The supported `--provider` registry. Indexed by provider name.
///
/// Endpoints were checked for reachability with curl. Each list endpoint answered without a key
/// (401 or 403), except Gemini's OpenAI-compatible path, which answered 404, so Gemini uses its
/// native list. Provider terms and paths can change, so the live test in this module should be run
/// after editing an entry.
pub(crate) const PROVIDER_CONFIGS: &[(&str, ProviderConfig)] = &[
    (
        "anthropic",
        ProviderConfig {
            base_url: "https://api.anthropic.com/v1/messages",
            key_env: Some("ANTHROPIC_API_KEY"),
            models_url: Some("https://api.anthropic.com/v1/models"),
            list: ModelList::Data,
        },
    ),
    (
        "openai",
        ProviderConfig {
            base_url: "https://api.openai.com/v1/chat/completions",
            key_env: Some("OPENAI_API_KEY"),
            models_url: Some("https://api.openai.com/v1/models"),
            list: ModelList::Data,
        },
    ),
    (
        "openrouter",
        ProviderConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions",
            key_env: Some("OPENROUTER_API_KEY"),
            models_url: Some("https://openrouter.ai/api/v1/models"),
            list: ModelList::Data,
        },
    ),
    (
        "grok",
        ProviderConfig {
            base_url: "https://api.x.ai/v1/chat/completions",
            key_env: Some("XAI_API_KEY"),
            models_url: Some("https://api.x.ai/v1/models"),
            list: ModelList::Data,
        },
    ),
    (
        "glm",
        // Zhipu's BigModel platform (GLM models), OpenAI-compatible.
        ProviderConfig {
            base_url: "https://open.bigmodel.cn/api/paas/v4/chat/completions",
            key_env: Some("ZHIPU_API_KEY"),
            models_url: Some("https://open.bigmodel.cn/api/paas/v4/models"),
            list: ModelList::Data,
        },
    ),
    (
        "deepseek",
        ProviderConfig {
            base_url: "https://api.deepseek.com/v1/chat/completions",
            key_env: Some("DEEPSEEK_API_KEY"),
            models_url: Some("https://api.deepseek.com/v1/models"),
            list: ModelList::Data,
        },
    ),
    (
        "qwen",
        // Alibaba Cloud Model Studio (DashScope), international endpoint, OpenAI-compatible.
        ProviderConfig {
            base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1/chat/completions",
            key_env: Some("DASHSCOPE_API_KEY"),
            models_url: Some("https://dashscope-intl.aliyuncs.com/compatible-mode/v1/models"),
            list: ModelList::Data,
        },
    ),
    (
        "gemini",
        // Gemini: chat through the OpenAI-compatibility layer, list through the native API.
        ProviderConfig {
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
            key_env: Some("GEMINI_API_KEY"),
            models_url: Some("https://generativelanguage.googleapis.com/v1beta/models"),
            list: ModelList::Gemini,
        },
    ),
    (
        "groq",
        ProviderConfig {
            base_url: "https://api.groq.com/openai/v1/chat/completions",
            key_env: Some("GROQ_API_KEY"),
            models_url: Some("https://api.groq.com/openai/v1/models"),
            list: ModelList::Data,
        },
    ),
    (
        "mistral",
        ProviderConfig {
            base_url: "https://api.mistral.ai/v1/chat/completions",
            key_env: Some("MISTRAL_API_KEY"),
            models_url: Some("https://api.mistral.ai/v1/models"),
            list: ModelList::Data,
        },
    ),
    (
        "together",
        ProviderConfig {
            base_url: "https://api.together.xyz/v1/chat/completions",
            key_env: Some("TOGETHER_API_KEY"),
            models_url: Some("https://api.together.xyz/v1/models"),
            list: ModelList::Array,
        },
    ),
    (
        "ollama",
        // Ollama exposes an OpenAI-compatible `/v1/chat/completions` endpoint, so
        // the generic provider impl works unchanged against it.
        ProviderConfig {
            base_url: "http://localhost:11434/v1/chat/completions",
            key_env: None,
            models_url: Some("http://localhost:11434/v1/models"),
            list: ModelList::Data,
        },
    ),
    (
        "lmstudio",
        // LM Studio's local server speaks the OpenAI chat-completions shape and needs no key.
        ProviderConfig {
            base_url: "http://localhost:1234/v1/chat/completions",
            key_env: None,
            models_url: Some("http://localhost:1234/v1/models"),
            list: ModelList::Data,
        },
    ),
    (
        "azure-openai",
        // Azure routes by deployment, so the endpoint is per-resource and must come from
        // `--base-url` (include the `?api-version=...` query). The key is sent as an `api-key`
        // header (see `openai_compat`), and `--model` names the deployment.
        ProviderConfig {
            base_url: "",
            key_env: Some("AZURE_OPENAI_API_KEY"),
            models_url: None,
            list: ModelList::None,
        },
    ),
];

/// Looks up a `--provider` name in [`PROVIDER_CONFIGS`].
pub(crate) fn provider_config(name: &str) -> Option<&'static ProviderConfig> {
    PROVIDER_CONFIGS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| c)
}

/// One model a provider lists. `created` is a sortable key: a zero-padded timestamp for
/// `created`, or the ISO string from `created_at`. Only comparisons within one provider use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub created: Option<String>,
}

/// Parses a provider's model-list response. Unknown shapes yield an empty list rather than an
/// error, so a changed response is reported as "no models" instead of crashing.
pub(crate) fn parse_models(list: ModelList, body: &serde_json::Value) -> Vec<ModelInfo> {
    let string_field =
        |v: &serde_json::Value, key: &str| v.get(key).and_then(|s| s.as_str()).map(str::to_string);
    match list {
        ModelList::Data => body
            .get("data")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let id = string_field(m, "id")?;
                let created = m
                    .get("created")
                    .and_then(|c| c.as_u64())
                    .map(|c| format!("{c:020}"))
                    .or_else(|| string_field(m, "created_at"));
                Some(ModelInfo { id, created })
            })
            .collect(),
        ModelList::Array => body
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some(ModelInfo {
                    id: string_field(m, "id")?,
                    created: None,
                })
            })
            .collect(),
        ModelList::Gemini => body
            .get("models")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
            .filter(|m| {
                // Keep only models that can generate text, when the response says which.
                m.get("supportedGenerationMethods")
                    .and_then(|g| g.as_array())
                    .is_none_or(|methods| {
                        methods
                            .iter()
                            .any(|x| x.as_str() == Some("generateContent"))
                    })
            })
            .filter_map(|m| {
                let name = string_field(m, "name")?;
                Some(ModelInfo {
                    id: name.strip_prefix("models/").unwrap_or(&name).to_string(),
                    created: None,
                })
            })
            .collect(),
        ModelList::None => Vec::new(),
    }
}

/// Substrings that mark a model as not a chat model. These are excluded before choosing one.
const NON_CHAT_MARKERS: &[&str] = &[
    "embed",
    "whisper",
    "tts",
    "transcri",
    "dall-e",
    "image",
    "moderation",
    "rerank",
    "audio",
    "realtime",
];

/// Chooses the model to use when `--model` is omitted. Among chat models, the one with the
/// newest `created` key wins. When no model has a comparable key, a single remaining chat model
/// is chosen. Otherwise nothing is chosen, and the caller should ask for `--model`.
pub fn choose_model(models: &[ModelInfo]) -> Option<String> {
    let chat: Vec<&ModelInfo> = models
        .iter()
        .filter(|m| {
            let lower = m.id.to_ascii_lowercase();
            !NON_CHAT_MARKERS.iter().any(|marker| lower.contains(marker))
        })
        .collect();
    // Routing variants (OpenRouter's `:free`, `:batch`) and local tags (Ollama's `llama3:8b`) use
    // `:`. Prefer the plain ids, and fall back to variants only when nothing else is listed.
    let plain: Vec<&ModelInfo> = chat
        .iter()
        .copied()
        .filter(|m| !m.id.contains(':'))
        .collect();
    let chat = if plain.is_empty() { chat } else { plain };
    if let Some(newest) = chat
        .iter()
        .filter_map(|m| m.created.as_ref().map(|c| (c, &m.id)))
        .max_by(|a, b| a.0.cmp(b.0))
    {
        return Some(newest.1.clone());
    }
    match chat.as_slice() {
        [only] => Some(only.id.clone()),
        _ => None,
    }
}

/// The URL that lists models. A `--base-url` override (a proxy or a self-hosted server) is
/// rewritten to its `/models` sibling, so the same server answers both requests.
pub(crate) fn models_endpoint(config: &ProviderConfig, base_url: Option<&str>) -> Option<String> {
    match base_url {
        Some(base) => {
            let stem = base
                .strip_suffix("/chat/completions")
                .or_else(|| base.strip_suffix("/messages"))?;
            Some(format!("{stem}/models"))
        }
        None => config.models_url.map(str::to_string),
    }
}

/// Lists the models `provider` offers to the configured key.
pub fn list_models(provider: &str, base_url: Option<&str>) -> anyhow::Result<Vec<ModelInfo>> {
    let config = provider_config(provider).ok_or_else(|| {
        let known: Vec<&str> = PROVIDER_CONFIGS.iter().map(|(n, _)| *n).collect();
        anyhow::anyhow!(
            "unknown --provider `{provider}`; supported: {}",
            known.join(", ")
        )
    })?;
    let url = models_endpoint(config, base_url).ok_or_else(|| {
        anyhow::anyhow!(
            "`{provider}` has no model list; pass --model (for Azure, the deployment name)"
        )
    })?;
    let key = match config.key_env {
        Some(env) => Some(std::env::var(env).with_context(|| {
            format!(
                "`{env}` is not set — export it before listing or using `{provider}` models \
                 (e.g. `export {env}=...`)"
            )
        })?),
        None => None,
    };

    let mut request = ureq::get(&url).set("accept", "application/json");
    match (provider, key.as_deref()) {
        ("anthropic", Some(key)) => {
            request = request
                .set("x-api-key", key)
                .set("anthropic-version", "2023-06-01");
        }
        ("gemini", Some(key)) => {
            request = request.set("x-goog-api-key", key);
        }
        (_, Some(key)) => {
            request = request.set("authorization", &format!("Bearer {key}"));
        }
        (_, None) => {}
    }

    let response = match request.call() {
        Ok(resp) => resp,
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            bail!("`{provider}` model list returned {code}: {body}");
        }
        Err(err) => {
            return Err(anyhow::Error::new(err).context(format!(
                "could not reach `{provider}`'s model list at {url}"
            )));
        }
    };
    let body: serde_json::Value = response
        .into_json()
        .with_context(|| format!("`{provider}` model list was not JSON"))?;
    let models = parse_models(config.list, &body);
    if models.is_empty() {
        bail!("`{provider}` listed no models (response shape changed, or the key has no access)");
    }
    Ok(models)
}

/// Picks the live model for `provider` when `--model` was not given, or explains what to pass.
fn resolve_live_model(provider: &str, base_url: Option<&str>) -> anyhow::Result<String> {
    let models = list_models(provider, base_url)?;
    match choose_model(&models) {
        Some(id) => {
            eprintln!(
                "note: no --model given, so using the newest chat model `{id}` from `{provider}` \
                 (pass --model to choose another)"
            );
            Ok(id)
        }
        None => {
            let shown: Vec<&str> = models.iter().take(15).map(|m| m.id.as_str()).collect();
            bail!(
                "`{provider}` lists {} models and none is clearly the newest chat model; pass \
                 --model. Examples: {}. See all with `tpt-appfront generate --list-models \
                 --provider {provider}`",
                models.len(),
                shown.join(", ")
            )
        }
    }
}

/// Resolves a `--provider` to a concrete [`LlmProvider`]. Validates the API key
/// requirement up front so a missing key is a clear error rather than a silent
/// fallback to the offline generator. `base_url` overrides the table default
/// (useful for self-hosted or proxy endpoints).
pub fn build_provider(
    provider: &str,
    model: &str,
    base_url: Option<&str>,
) -> anyhow::Result<Box<dyn LlmProvider>> {
    let config = provider_config(provider).ok_or_else(|| {
        let known: Vec<&str> = PROVIDER_CONFIGS.iter().map(|(n, _)| *n).collect();
        anyhow::anyhow!(
            "unknown --provider `{provider}`; supported: {}",
            known.join(", ")
        )
    })?;

    // Providers with no built-in endpoint (Azure: per-resource) must be given one.
    if config.base_url.is_empty() && base_url.is_none() {
        bail!("`--provider {provider}` requires `--base-url` (the endpoint for your resource/deployment)");
    }
    let endpoint = base_url.unwrap_or(config.base_url);

    // An explicit `--model` wins. Otherwise the model is chosen from the live list.
    let model = if model.is_empty() {
        resolve_live_model(provider, base_url)?
    } else {
        model.to_string()
    };

    // Resolve the API key (key-less providers skip this).
    let api_key = match config.key_env {
        Some(env) => Some(std::env::var(env).with_context(|| {
            format!(
                "`{env}` is not set — export it before using `generate --llm` with the \
                 `{provider}` provider (e.g. `export {env}=...`)"
            )
        })?),
        None => None,
    };

    if provider == "anthropic" {
        Ok(Box::new(anthropic::AnthropicProvider::new(
            api_key.expect("anthropic has a key_env"),
            model,
            endpoint,
        )))
    } else {
        Ok(Box::new(openai_compat::OpenAiCompatProvider::new(
            provider, api_key, model, endpoint,
        )))
    }
}

/// Prints the live model list for `provider`, newest first when the provider reports creation
/// times. Used by `generate --list-models`.
pub fn print_models(provider: &str, base_url: Option<&str>) -> anyhow::Result<()> {
    let mut models = list_models(provider, base_url)?;
    models.sort_by(|a, b| b.created.cmp(&a.created).then(a.id.cmp(&b.id)));
    println!("Models listed by `{provider}` ({} total):", models.len());
    for m in &models {
        println!("  {}", m.id);
    }
    match choose_model(&models) {
        Some(id) => println!("\nWithout --model, `generate` would use: {id}"),
        None => println!("\nNo single newest chat model; pass --model to choose one."),
    }
    Ok(())
}

/// Builds the system prompt from the live `view!` tag set and few-shot examples.
/// Declared `pub(crate)` so the tests can assert it tracks [`VIEW_TAGS`].
pub(crate) fn build_system_prompt() -> String {
    let tags = VIEW_TAGS
        .iter()
        .map(|t| format!("  - {t}"))
        .collect::<Vec<_>>()
        .join("\n");

    let examples = PATTERNS
        .iter()
        .map(|p| format!("### {}\n```rust\n{}\n```", p.name, p.snippet))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        "You are a UI scaffolding assistant for the `tpt-appfront` Rust framework. \
         You write a single `view!` macro expression (re-exported as `tpt_appfront_core::view!`) \
         that builds a `UITree<Msg>`. Emit ONLY a Rust code block containing the `view!` call; \
         no surrounding explanation.

Supported `view!` tags (each maps to a `tpt-appfront-core` `NodeKind`):
{tags}

Rules:
- The root must be a `<Container>`. Buttons use `<Button on_click={{Msg::X}}>\"label\"</Button>`; \
  for two-way binding, `<Input value=\"..\" on_input={{Msg::Set(v)}} />`.
- Wire every `on_click`/`on_input` to a `Msg` variant from the app's `Msg` enum. \
  Invent reasonable `Msg` variants if the prompt implies them; the user wires them into their enum.
- Prefer the prebuilt `tpt_appfront_templates` pieces (`login_form`, `dashboard_shell`, \
  `settings_list`) for login/dashboard/settings shapes, but you may inline a `view!` equivalent.
- Keep the snippet self-contained and compilable against `tpt_appfront_core::view!`.

Few-shot examples (match their style and completeness):

{examples}
"
    )
}

/// Extracts the first fenced ```rust (or ```) code block from an LLM reply.
/// Falls back to `None` when there is no fence at all (the caller then decides
/// how to treat the raw text).
pub(crate) fn extract_code_block(text: &str) -> Option<String> {
    let mut start = None;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") && start.is_none() {
            start = Some(i + 1);
            continue;
        }
        if let Some(open) = start {
            if trimmed.starts_with("```") {
                return Some(text.lines().collect::<Vec<_>>()[open..i].join("\n"));
            }
        }
    }
    // No closing fence: if we saw an opening fence, return everything after it.
    if let Some(open) = start {
        return Some(text.lines().skip(open).collect::<Vec<_>>().join("\n"));
    }
    None
}

/// Syntax-checks Rust source without compiling it, via `syn`. Returns the parse
/// error message on failure. Used to validate an LLM's `view!` output before we
/// hand it to the user (never a hard error. The CLI prints a `// WARNING:`
/// banner and still emits the snippet).
pub(crate) fn validate_rs_syntax(code: &str) -> Result<(), String> {
    syn::parse_file(code).map(|_| ()).map_err(|e| e.to_string())
}

/// Runs the full LLM generate flow: build the prompt, call the provider, extract
/// and syntax-check the code block, and return a ready-to-print snippet.
///
/// On a syntax-validation failure the snippet is still returned, prefixed with a
/// `// WARNING:` banner, so a close-but-imperfect model response is usable.
pub fn generate(
    prompt: &str,
    provider_name: &str,
    model: &str,
    base_url: Option<&str>,
) -> anyhow::Result<String> {
    let provider = build_provider(provider_name, model, base_url)?;
    let system = build_system_prompt();
    let user = format!(
        "Scaffold a `view!` UI for this request:\n\n{prompt}\n\nEmit only the `view!` code block."
    );

    let reply = provider
        .complete(&system, &user)
        .with_context(|| format!("LLM completion via `{}` failed", provider.name()))?;

    let code = extract_code_block(&reply).unwrap_or_else(|| reply.clone());

    let banner = match validate_rs_syntax(&code) {
        Ok(()) => String::new(),
        Err(err) => format!(
            "// WARNING: the generated snippet failed Rust syntax validation ({err}).\n\
             // It is emitted as-is for you to fix by hand rather than discarded.\n"
        ),
    };

    Ok(format!(
        "// Generated by `tpt-appfront generate --prompt \"{prompt}\" --llm`\n\
         // via provider `{provider_name}` (model `{}`). Wire the emitted Msg\n\
         // variants into your app's Msg enum and update loop.\n{banner}{code}\n",
        provider.model()
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn system_prompt_includes_every_live_tag() {
        let prompt = build_system_prompt();
        for tag in VIEW_TAGS {
            assert!(
                prompt.contains(tag),
                "system prompt must mention tag `{tag}`"
            );
        }
    }

    #[test]
    fn system_prompt_reuses_pattern_few_shots() {
        let prompt = build_system_prompt();
        for p in PATTERNS {
            assert!(
                prompt.contains(p.name),
                "system prompt should reuse pattern `{}`",
                p.name
            );
        }
    }

    /// Providers that need no API key (local servers).
    const KEYLESS: &[&str] = &["ollama", "lmstudio"];

    #[test]
    fn provider_table_is_consistent() {
        let mut seen = std::collections::HashSet::new();
        for (name, cfg) in PROVIDER_CONFIGS {
            assert!(seen.insert(*name), "duplicate provider `{name}`");
            assert_eq!(
                KEYLESS.contains(name),
                cfg.key_env.is_none(),
                "{name}: key_env and the KEYLESS list disagree"
            );
            if let Some(env) = cfg.key_env {
                assert!(
                    env.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                    "{name}: env var `{env}` should be an upper-case identifier"
                );
            }
            // Every provider that can list models says where, and the rest say so explicitly.
            assert_eq!(
                cfg.models_url.is_none(),
                cfg.list == ModelList::None,
                "{name}: models_url and the list shape disagree"
            );
            if let Some(url) = cfg.models_url {
                assert!(
                    url.starts_with("https://") || url.starts_with("http://"),
                    "{name}: {url}"
                );
            }
            if *name == "azure-openai" {
                assert!(cfg.base_url.is_empty(), "azure has no built-in endpoint");
            } else {
                assert!(
                    cfg.base_url.starts_with("https://") || cfg.base_url.starts_with("http://"),
                    "{name}: base URL `{}` must be http(s)",
                    cfg.base_url
                );
            }
        }
    }

    #[test]
    fn provider_table_covers_the_requested_providers() {
        for name in ["glm", "deepseek", "qwen", "gemini", "anthropic", "openai"] {
            assert!(provider_config(name).is_some(), "{name} is in the table");
        }
    }

    #[test]
    fn parse_models_reads_the_data_shape() {
        let body = json!({"data": [
            {"id": "gpt-old", "created": 1700000000},
            {"id": "gpt-new", "created": 1800000000}
        ]});
        let models = parse_models(ModelList::Data, &body);
        assert_eq!(models.len(), 2);
        assert_eq!(models[1].id, "gpt-new");
        assert!(models[1].created > models[0].created);
    }

    #[test]
    fn parse_models_reads_anthropic_created_at() {
        let body = json!({"data": [
            {"id": "claude-a", "created_at": "2025-01-01T00:00:00Z"},
            {"id": "claude-b", "created_at": "2026-02-01T00:00:00Z"}
        ]});
        let models = parse_models(ModelList::Data, &body);
        assert_eq!(choose_model(&models).as_deref(), Some("claude-b"));
    }

    #[test]
    fn parse_models_reads_the_bare_array_shape() {
        let body = json!([{"id": "meta/llama"}, {"id": "mistral/x"}]);
        let models = parse_models(ModelList::Array, &body);
        assert_eq!(
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["meta/llama", "mistral/x"]
        );
    }

    #[test]
    fn parse_models_reads_gemini_and_keeps_only_text_generators() {
        let body = json!({"models": [
            {"name": "models/gemini-2.5-flash", "supportedGenerationMethods": ["generateContent"]},
            {"name": "models/text-embedding-004", "supportedGenerationMethods": ["embedContent"]}
        ]});
        let models = parse_models(ModelList::Gemini, &body);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gemini-2.5-flash");
    }

    #[test]
    fn unknown_shapes_list_nothing_rather_than_failing() {
        assert!(parse_models(ModelList::Data, &json!({"oops": 1})).is_empty());
        assert!(parse_models(ModelList::None, &json!({"data": [{"id": "x"}]})).is_empty());
    }

    #[test]
    fn choose_model_skips_non_chat_models() {
        let models = vec![
            ModelInfo {
                id: "text-embedding-3".into(),
                created: Some("900".into()),
            },
            ModelInfo {
                id: "whisper-1".into(),
                created: Some("950".into()),
            },
            ModelInfo {
                id: "chat-small".into(),
                created: Some("100".into()),
            },
            ModelInfo {
                id: "chat-large".into(),
                created: Some("200".into()),
            },
        ];
        assert_eq!(choose_model(&models).as_deref(), Some("chat-large"));
    }

    #[test]
    fn choose_model_takes_the_single_chat_model_without_timestamps() {
        let models = vec![
            ModelInfo {
                id: "only-chat".into(),
                created: None,
            },
            ModelInfo {
                id: "some-embed".into(),
                created: None,
            },
        ];
        assert_eq!(choose_model(&models).as_deref(), Some("only-chat"));
    }

    #[test]
    fn choose_model_prefers_plain_ids_over_routing_variants() {
        let models = vec![
            ModelInfo {
                id: "vendor/model-a:free".into(),
                created: Some("300".into()),
            },
            ModelInfo {
                id: "vendor/model-a".into(),
                created: Some("200".into()),
            },
            ModelInfo {
                id: "vendor/model-b:batch".into(),
                created: Some("400".into()),
            },
        ];
        assert_eq!(choose_model(&models).as_deref(), Some("vendor/model-a"));
    }

    #[test]
    fn choose_model_still_uses_a_tagged_local_model_when_it_is_the_only_one() {
        // Ollama lists tags such as `llama3.1:8b`. With no plain ids, a tagged one is still chosen.
        let models = vec![ModelInfo {
            id: "llama3.1:8b".into(),
            created: None,
        }];
        assert_eq!(choose_model(&models).as_deref(), Some("llama3.1:8b"));
    }

    #[test]
    fn choose_model_refuses_to_guess_among_untimestamped_models() {
        let models = vec![
            ModelInfo {
                id: "a".into(),
                created: None,
            },
            ModelInfo {
                id: "b".into(),
                created: None,
            },
        ];
        assert_eq!(choose_model(&models), None);
    }

    #[test]
    fn models_endpoint_follows_a_base_url_override() {
        let openai = provider_config("openai").unwrap();
        assert_eq!(
            models_endpoint(openai, Some("http://proxy.local/v1/chat/completions")).as_deref(),
            Some("http://proxy.local/v1/models")
        );
        assert_eq!(
            models_endpoint(openai, None).as_deref(),
            Some("https://api.openai.com/v1/models")
        );
        let anthropic = provider_config("anthropic").unwrap();
        assert_eq!(
            models_endpoint(anthropic, Some("http://proxy.local/v1/messages")).as_deref(),
            Some("http://proxy.local/v1/models")
        );
        assert_eq!(
            models_endpoint(provider_config("azure-openai").unwrap(), None),
            None
        );
    }

    #[test]
    fn build_provider_rejects_unknown_provider() {
        assert!(build_provider("nope", "", None).is_err());
    }

    #[test]
    fn explicit_model_skips_the_live_lookup() {
        // An explicit model needs no network call, so this works without a server or a key.
        assert!(build_provider("ollama", "llama3.1", None).is_ok());
        assert!(build_provider("lmstudio", "qwen2.5-coder", None).is_ok());
    }

    #[test]
    fn azure_requires_base_url_before_anything_else() {
        let err = build_provider("azure-openai", "my-deployment", None)
            .err()
            .expect("azure needs --base-url");
        assert!(err.to_string().contains("--base-url"), "got: {err}");
    }

    #[test]
    fn azure_without_a_deployment_asks_for_model() {
        // No list endpoint and no model: the error names the deployment, and the endpoint check
        // passes because a base URL is given.
        let err = build_provider("azure-openai", "", Some("https://r.example/x"))
            .err()
            .expect("azure needs a model");
        assert!(err.to_string().contains("--model"), "got: {err}");
    }

    #[test]
    fn missing_key_error_names_the_env_var_before_any_request() {
        // Only meaningful when the variable is unset on this machine.
        if std::env::var("GROQ_API_KEY").is_ok() {
            return;
        }
        let err = list_models("groq", None).expect_err("groq needs GROQ_API_KEY");
        assert!(err.to_string().contains("GROQ_API_KEY"), "got: {err}");
    }

    #[test]
    fn extract_code_block_pulls_the_fenced_rust_block() {
        let reply = "Here is your UI:\n```rust\ntpt_appfront_core::view! { <Container/> }\n```\nLet me know!";
        let code = extract_code_block(reply).expect("expected a code block");
        assert!(code.contains("view!"));
        assert!(!code.contains("Here is your UI"));
        assert!(!code.contains("Let me know"));
    }

    #[test]
    fn extract_code_block_handles_prose_wrapped_without_lang_token() {
        let reply = "Sure.\n```\nsome_code()\n```";
        let code = extract_code_block(reply).expect("expected a code block");
        assert_eq!(code, "some_code()");
    }

    #[test]
    fn extract_code_block_returns_none_when_no_fence() {
        assert!(extract_code_block("just prose, no code").is_none());
    }

    #[test]
    fn extract_code_block_takes_first_block_when_multiple() {
        let reply = "```rust\na\n```\n```rust\nb\n```";
        assert_eq!(extract_code_block(reply).unwrap(), "a");
    }

    #[test]
    fn validate_rs_syntax_accepts_view_macro_snippet() {
        let snippet = r#"tpt_appfront_core::view! {
            <Container class="app">
                <Heading level={1u8}>"Hi"</Heading>
            </Container>
        }"#;
        assert!(validate_rs_syntax(snippet).is_ok());
    }

    #[test]
    fn validate_rs_syntax_rejects_garbage() {
        assert!(validate_rs_syntax("fn this is not rust (((").is_err());
    }

    #[test]
    fn generate_emits_warning_banner_on_malformed_response() {
        // `generate` calls the network via `build_provider`. Test the banner logic directly.
        let code = "fn not valid (((";
        let banner = match validate_rs_syntax(code) {
            Ok(()) => String::new(),
            Err(err) => format!(
                "// WARNING: the generated snippet failed Rust syntax validation ({err}).\n"
            ),
        };
        assert!(banner.contains("WARNING"));
    }

    /// Live checks. They never run in normal `cargo test` (CI has no keys and must stay
    /// network-free). Run locally with `cargo test --features llm -- --ignored` after exporting the
    /// provider's key. Each one lists the live models and checks that a chat model can be chosen.
    #[test]
    #[ignore]
    fn live_list_and_choose_for_each_keyed_provider() {
        for provider in [
            "anthropic",
            "openai",
            "openrouter",
            "grok",
            "glm",
            "deepseek",
            "qwen",
            "gemini",
            "groq",
            "mistral",
            "together",
        ] {
            let models =
                list_models(provider, None).unwrap_or_else(|e| panic!("{provider}: {e:#}"));
            assert!(!models.is_empty(), "{provider} lists models");
            if choose_model(&models).is_none() {
                eprintln!("{provider}: no single newest chat model; --model is needed");
            }
        }
    }

    /// End to end with a real key. Lists models, picks one, and asks for a snippet.
    #[test]
    #[ignore]
    fn live_generate_with_the_chosen_model() {
        let out = generate(
            "a minimal counter with +1 and -1 buttons",
            "anthropic",
            "",
            None,
        )
        .expect("live generate failed");
        assert!(
            out.contains("view!"),
            "expected a view! snippet, got:\n{out}"
        );
    }
}
