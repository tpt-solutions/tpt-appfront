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
//!   (`PROVIDER_CONFIGS`) — adding OpenAI/OpenRouter/Grok/Ollama was a data
//!   change, not a new code path, and new providers follow the same pattern.
//! - The system prompt is derived from `view!`'s supported tag set (mirrored in
//!   [`VIEW_TAGS`], see its doc comment for the proc-macro-crate constraint)
//!   plus few-shot examples reused from `crate::generate::PATTERNS` — both are
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
/// a dependency cycle). Keep the two lists in sync — `build_system_prompt`'s
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
    /// Run the completion. Network/API errors surface as [`anyhow::Error`];
    /// the provider must give a clear message (e.g. a missing API key) rather
    /// than silently falling back to the offline generator.
    fn complete(&self, system: &str, user: &str) -> anyhow::Result<String>;
}

/// Static configuration for each `--provider` choice. `openai_compat` providers
/// share one request/response shape; only these fields differ, so adding a
/// provider is a one-line table entry plus (if non-OpenAI-compatible) a provider
/// impl — no CLI-surface changes.
pub(crate) struct ProviderConfig {
    /// Default endpoint for this provider's `chat/completions` (or Messages) API.
    pub base_url: &'static str,
    /// Env var holding the API key, or `None` for key-less local providers (Ollama).
    pub key_env: Option<&'static str>,
    /// Model id used when `--model` is not given. Empty means the provider
    /// requires an explicit `--model` (Ollama).
    pub default_model: &'static str,
}

/// The supported `--provider` registry. Indexed by provider name.
pub(crate) const PROVIDER_CONFIGS: &[(&str, ProviderConfig)] = &[
    (
        "anthropic",
        ProviderConfig {
            base_url: "https://api.anthropic.com/v1/messages",
            key_env: Some("ANTHROPIC_API_KEY"),
            default_model: "claude-sonnet-5-5",
        },
    ),
    (
        "openai",
        ProviderConfig {
            base_url: "https://api.openai.com/v1/chat/completions",
            key_env: Some("OPENAI_API_KEY"),
            default_model: "gpt-4o",
        },
    ),
    (
        "openrouter",
        ProviderConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions",
            key_env: Some("OPENROUTER_API_KEY"),
            default_model: "openai/gpt-4o",
        },
    ),
    (
        "grok",
        ProviderConfig {
            base_url: "https://api.x.ai/v1/chat/completions",
            key_env: Some("XAI_API_KEY"),
            default_model: "grok-3",
        },
    ),
    (
        "ollama",
        // Ollama exposes an OpenAI-compatible `/v1/chat/completions` endpoint, so
        // the generic provider impl works unchanged against it.
        ProviderConfig {
            base_url: "http://localhost:11434/v1/chat/completions",
            key_env: None,
            default_model: "",
        },
    ),
    (
        "gemini",
        // Google's OpenAI-compatibility layer for the Gemini API.
        ProviderConfig {
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
            key_env: Some("GEMINI_API_KEY"),
            default_model: "",
        },
    ),
    (
        "groq",
        ProviderConfig {
            base_url: "https://api.groq.com/openai/v1/chat/completions",
            key_env: Some("GROQ_API_KEY"),
            default_model: "",
        },
    ),
    (
        "mistral",
        ProviderConfig {
            base_url: "https://api.mistral.ai/v1/chat/completions",
            key_env: Some("MISTRAL_API_KEY"),
            default_model: "",
        },
    ),
    (
        "deepseek",
        ProviderConfig {
            base_url: "https://api.deepseek.com/v1/chat/completions",
            key_env: Some("DEEPSEEK_API_KEY"),
            default_model: "",
        },
    ),
    (
        "together",
        ProviderConfig {
            base_url: "https://api.together.xyz/v1/chat/completions",
            key_env: Some("TOGETHER_API_KEY"),
            default_model: "",
        },
    ),
    (
        "lmstudio",
        // LM Studio's local server speaks the OpenAI chat-completions shape and needs no key.
        ProviderConfig {
            base_url: "http://localhost:1234/v1/chat/completions",
            key_env: None,
            default_model: "",
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
            default_model: "",
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

    // Resolve the model id: explicit `--model` wins; otherwise the provider's
    // default. Ollama has no default, so it must be supplied explicitly.
    let model = if !model.is_empty() {
        model.to_string()
    } else if !config.default_model.is_empty() {
        config.default_model.to_string()
    } else {
        bail!("`--provider {provider}` requires an explicit `--model` (e.g. `--model llama3.1`)");
    };

    // Providers with no built-in endpoint (Azure: per-resource) must be given one.
    if config.base_url.is_empty() && base_url.is_none() {
        bail!("`--provider {provider}` requires `--base-url` (the endpoint for your resource/deployment)");
    }
    let endpoint = base_url.unwrap_or(config.base_url);

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
            provider,
            api_key,
            model,
            endpoint,
        )))
    }
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
/// hand it to the user (never a hard error — the CLI prints a `// WARNING:`
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
         // via provider `{provider_name}` (model `{model}`). Wire the emitted Msg\n\
         // variants into your app's Msg enum and update loop.\n{banner}{code}\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_includes_every_live_tag() {
        let prompt = build_system_prompt();
        for tag in VIEW_TAGS {
            assert!(prompt.contains(tag), "system prompt must mention tag `{tag}`");
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
    /// Providers with no default model id, so `--model` must be given. Defaults are left out
    /// on purpose for these: model ids change too often to pin without checking each provider.
    const EXPLICIT_MODEL: &[&str] = &[
        "ollama",
        "lmstudio",
        "gemini",
        "groq",
        "mistral",
        "deepseek",
        "together",
        "azure-openai",
    ];

    #[test]
    fn provider_config_table_is_consistent() {
        let mut seen = std::collections::HashSet::new();
        for (name, cfg) in PROVIDER_CONFIGS {
            assert!(seen.insert(*name), "duplicate provider `{name}`");
            // Every entry must be classified above, so a new provider can't skip these rules.
            assert!(
                KEYLESS.contains(name) || cfg.key_env.is_some(),
                "{name} needs an API key env or a KEYLESS entry"
            );
            assert_eq!(
                KEYLESS.contains(name),
                cfg.key_env.is_none(),
                "{name}: key_env and the KEYLESS list disagree"
            );
            assert_eq!(
                EXPLICIT_MODEL.contains(name),
                cfg.default_model.is_empty(),
                "{name}: default_model and the EXPLICIT_MODEL list disagree"
            );
            if let Some(env) = cfg.key_env {
                assert!(
                    env.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                    "{name}: env var `{env}` should be an upper-case identifier"
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
    fn build_provider_rejects_unknown_provider() {
        // Ollama needs no key, so it resolves without env vars set.
        assert!(build_provider("ollama", "llama3.1", None).is_ok());
        assert!(build_provider("nope", "", None).is_err());
    }

    #[test]
    fn build_provider_requires_explicit_model_where_there_is_no_default() {
        for name in EXPLICIT_MODEL {
            let err = build_provider(name, "", Some("http://127.0.0.1:9"))
                .err()
                .unwrap_or_else(|| panic!("{name} should require --model"));
            assert!(err.to_string().contains("--model"), "{name}: got {err}");
        }
    }

    #[test]
    fn build_provider_keyless_local_providers_resolve_without_env() {
        assert!(build_provider("ollama", "llama3.1", None).is_ok());
        assert!(build_provider("lmstudio", "qwen2.5-coder", None).is_ok());
    }

    #[test]
    fn build_provider_azure_requires_base_url() {
        let err = build_provider("azure-openai", "my-deployment", None)
            .err()
            .expect("azure needs --base-url");
        assert!(err.to_string().contains("--base-url"), "got: {err}");
    }

    #[test]
    fn build_provider_missing_key_error_names_the_env_var() {
        // Only meaningful when the variable is unset on this machine.
        if std::env::var("GROQ_API_KEY").is_ok() {
            return;
        }
        let err = build_provider("groq", "llama-3.3-70b", None)
            .err()
            .expect("groq needs GROQ_API_KEY");
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
        // `generate` calls the network via `build_provider`; test the banner
        // logic directly through the lower-level pieces.
        let code = "fn not valid (((";
        let banner = match validate_rs_syntax(code) {
            Ok(()) => String::new(),
            Err(err) => format!("// WARNING: the generated snippet failed Rust syntax validation ({err}).\n"),
        };
        assert!(banner.contains("WARNING"));
    }

    /// Real-API smoke test. Never runs in normal `cargo test` (CI has no key and
    /// must stay network-free); run locally with `cargo test --features llm \
    /// generate_against_real_api -- --ignored` after exporting the provider's key.
    #[test]
    #[ignore]
    fn generate_against_real_api() {
        let out = generate(
            "a minimal counter with +1 and -1 buttons",
            "anthropic",
            "claude-sonnet-4-5",
            None,
        )
        .expect("live API call failed");
        assert!(out.contains("view!"), "expected a view! snippet, got:\n{out}");
        assert!(out.contains("Button"), "expected buttons in the snippet");
    }

    /// Per-provider real-API smoke tests (all `#[ignore]`d, never wired into CI).
    /// Run locally with `cargo test --features llm -- --ignored` after exporting
    /// each provider's key env var (Ollama also needs `--model`/a running server).
    #[test]
    #[ignore]
    fn generate_against_openai() {
        let out = generate("a minimal counter", "openai", "gpt-4o", None).expect("openai call failed");
        assert!(out.contains("view!"));
    }

    #[test]
    #[ignore]
    fn generate_against_openrouter() {
        let out = generate("a minimal counter", "openrouter", "openai/gpt-4o", None)
            .expect("openrouter call failed");
        assert!(out.contains("view!"));
    }

    #[test]
    #[ignore]
    fn generate_against_grok() {
        let out = generate("a minimal counter", "grok", "grok-3", None).expect("grok call failed");
        assert!(out.contains("view!"));
    }

    #[test]
    #[ignore]
    fn generate_against_ollama() {
        let out = generate("a minimal counter", "ollama", "llama3.1", None)
            .expect("ollama call failed (is `ollama serve` running?)");
        assert!(out.contains("view!"));
    }
}
