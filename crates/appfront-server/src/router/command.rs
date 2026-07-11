//! Inbound command API types for `POST /command` — deserialized instructions
//! from AI agents / automated clients and their responses.

use std::collections::HashMap;

/// Default steady-state rate and burst allowance for `POST /command` when the
/// app doesn't configure its own via [`crate::SmartRouterBuilder::rate_limit`]
/// — a route with an app-supplied handler shouldn't ship unthrottled by
/// default.
const DEFAULT_COMMAND_RATE_PER_SECOND: u64 = 5;
const DEFAULT_COMMAND_RATE_BURST: u32 = 10;

/// An inbound instruction from an AI agent or other automated client,
/// deserialized from the `POST /command` request body.
///
/// `action` must match a node's `AiMeta::action` (the same name
/// [`appfront_core::trigger_event`] matches against) and, if the router was
/// configured with [`crate::SmartRouterBuilder::allowed_actions`], must appear
/// in that allowlist.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Command {
    pub action: String,
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
}

/// Result of executing a [`Command`], returned as the `POST /command`
/// response body.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CommandResponse {
    pub ok: bool,
    pub message: String,
}

impl CommandResponse {
    pub fn ok(message: impl Into<String>) -> Self {
        CommandResponse { ok: true, message: message.into() }
    }

    pub fn err(message: impl Into<String>) -> Self {
        CommandResponse { ok: false, message: message.into() }
    }
}

/// App-supplied callback that executes a [`Command`] — typically by calling
/// [`appfront_core::trigger_event`] or [`appfront_core::navigate_to`] against
/// the app's own reactive state and `Msg` dispatch.
pub type CommandHandler = dyn Fn(Command) -> CommandResponse + Send + Sync;

/// Rate limit applied to `POST /command`. The limit is a single shared bucket
/// for the whole route (not per-client): this router has no connect-info
/// plumbing yet, so per-IP limiting isn't available — see
/// [`crate::SmartRouterBuilder::rate_limit`].
#[derive(Debug, Clone, Copy)]
pub struct RateLimitConfig {
    /// Sustained requests per second once the burst allowance is exhausted.
    pub per_second: u64,
    /// Number of requests permitted in a burst above the steady-state rate.
    pub burst: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        RateLimitConfig {
            per_second: DEFAULT_COMMAND_RATE_PER_SECOND,
            burst: DEFAULT_COMMAND_RATE_BURST,
        }
    }
}
