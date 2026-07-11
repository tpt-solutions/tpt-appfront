//! Axum-based smart router that detects client type and serves the
//! appropriate rendering backend.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use appfront_core::{HydrationPayload, UITree};
use axum::extract::{DefaultBodyLimit, Query};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::GlobalKeyExtractor;
use tower_governor::GovernorLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::client_kind::{self, ClientKind};
use crate::pwa::{manifest, manifest_link, registration_script, service_worker, PwaConfig};

/// Maximum accepted body size for `POST /command`, applied before the
/// request reaches the handler — closes the gap noted in Phase 12: no
/// write route should ship without a body-size limit from day one.
const COMMAND_BODY_LIMIT_BYTES: usize = 16 * 1024;

/// Default steady-state rate and burst allowance for `POST /command` when the
/// app doesn't configure its own via [`SmartRouterBuilder::rate_limit`] — a
/// route with an app-supplied handler shouldn't ship unthrottled by default,
/// same precedent as [`COMMAND_BODY_LIMIT_BYTES`].
const DEFAULT_COMMAND_RATE_PER_SECOND: u64 = 5;
const DEFAULT_COMMAND_RATE_BURST: u32 = 10;

/// An inbound instruction from an AI agent or other automated client,
/// deserialized from the `POST /command` request body.
///
/// `action` must match a node's `AiMeta::action` (the same name
/// [`appfront_core::trigger_event`] matches against) and, if the router was
/// configured with [`SmartRouterBuilder::allowed_actions`], must appear in
/// that allowlist.
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
/// [`SmartRouterBuilder::rate_limit`].
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

/// Configuration for the smart router.
pub struct SmartRouter<Msg> {
    /// The application's UITree, used for SSR / AI-Schema rendering.
    pub ui: UITree<Msg>,
    /// Page title for HTML / OpenGraph.
    pub title: String,
    /// Page description for HTML / OpenGraph.
    pub description: String,
    /// Directory holding static assets (WASM, JS, CSS, etc.).
    pub static_dir: PathBuf,
    /// The WASM shell HTML template. `{title}`, `{description}`, and
    /// `{wasm_path}` are replaced at serve time.
    pub wasm_shell_template: String,
    /// Path (relative to the server root) at which the WASM binary is served.
    pub wasm_path: String,
    /// When `true`, human browsers receive a hydration-ready page: SSR HTML
    /// with `data-appfront-id` attributes, a serialised `HydrationPayload`,
    /// and the WASM script. The client then calls [`hydrate`] instead of
    /// [`mount`][appfront_dom::mount].
    pub enable_hydration: bool,
    /// Named signal values carried in the hydration payload so that
    /// `Signal::hydrated("name", default)` on the client can restore
    /// the server-side state.
    pub signals: HashMap<String, serde_json::Value>,
    /// App-supplied handler for `POST /command`. `None` means the route
    /// responds `501 Not Implemented` — the endpoint exists but is inert
    /// until the app wires up a handler.
    pub command_handler: Option<std::sync::Arc<CommandHandler>>,
    /// If set, `POST /command` rejects any `action` not in this list with
    /// `403 Forbidden` before invoking `command_handler`.
    pub allowed_actions: Option<Vec<String>>,
    /// Rate limit applied to `POST /command`. Always on; defaults to
    /// [`RateLimitConfig::default`] if not overridden via
    /// [`SmartRouterBuilder::rate_limit`].
    pub rate_limit: RateLimitConfig,
    /// When `Some`, the app is served as a PWA: the router serves
    /// `/service-worker.js` + `/manifest.webmanifest` and injects the
    /// manifest `<link>` + service-worker registration `<script>` into the
    /// HTML shells. Off by default.
    pub pwa: Option<PwaConfig>,
}

/// Builder-pattern helper for constructing a [`SmartRouter`] with sensible
/// defaults.
pub struct SmartRouterBuilder<Msg> {
    ui: UITree<Msg>,
    title: String,
    description: String,
    static_dir: PathBuf,
    wasm_path: String,
    enable_hydration: bool,
    signals: HashMap<String, serde_json::Value>,
    command_handler: Option<std::sync::Arc<CommandHandler>>,
    allowed_actions: Option<Vec<String>>,
    rate_limit: RateLimitConfig,
    pwa: Option<PwaConfig>,
}

impl<Msg> SmartRouterBuilder<Msg> {
    pub fn new(ui: UITree<Msg>) -> Self {
        SmartRouterBuilder {
            ui,
            title: String::new(),
            description: String::new(),
            static_dir: PathBuf::from("dist"),
            wasm_path: "/app.wasm".to_string(),
            enable_hydration: false,
            signals: HashMap::new(),
            command_handler: None,
            allowed_actions: None,
            rate_limit: RateLimitConfig::default(),
            pwa: None,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn static_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.static_dir = dir.into();
        self
    }

    pub fn wasm_path(mut self, path: impl Into<String>) -> Self {
        self.wasm_path = path.into();
        self
    }

    pub fn enable_hydration(mut self, enabled: bool) -> Self {
        self.enable_hydration = enabled;
        self
    }

    pub fn signals(mut self, signals: HashMap<String, serde_json::Value>) -> Self {
        self.signals = signals;
        self
    }

    /// Wire up `POST /command` to execute inbound agent commands. Typically
    /// the closure calls [`appfront_core::trigger_event`]/
    /// [`appfront_core::navigate_to`] against the app's own state.
    pub fn on_command(
        mut self,
        handler: impl Fn(Command) -> CommandResponse + Send + Sync + 'static,
    ) -> Self {
        self.command_handler = Some(std::sync::Arc::new(handler));
        self
    }

    /// Restrict `POST /command` to only these `action` names. Any other
    /// action is rejected with `403` before `command_handler` runs.
    pub fn allowed_actions(mut self, actions: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.allowed_actions = Some(actions.into_iter().map(Into::into).collect());
        self
    }

    /// Override the default rate limit ([`RateLimitConfig::default`]) applied
    /// to `POST /command`.
    pub fn rate_limit(mut self, rate_limit: RateLimitConfig) -> Self {
        self.rate_limit = rate_limit;
        self
    }

    /// Turn the app into an installable, offline-capable PWA. Serves
    /// `/service-worker.js` + `/manifest.webmanifest` and injects the
    /// manifest `<link>` + registration `<script>` into the HTML shells.
    pub fn pwa(mut self, config: PwaConfig) -> Self {
        self.pwa = Some(config);
        self
    }

    pub fn build(self) -> SmartRouter<Msg> {
        let wasm_shell_template = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{TITLE}</title>
<meta property="og:title" content="{TITLE}">
<meta property="og:description" content="{DESC}">
<meta property="og:type" content="website">
</head>
<body>
<div id="appfront-root"></div>
<script type="module">
import init from '{WASM}';
init().catch(e => console.error('appfront init failed', e));
</script>
</body>
</html>
"#
        .to_string();

        SmartRouter {
            ui: self.ui,
            title: self.title,
            description: self.description,
            static_dir: self.static_dir,
            wasm_shell_template,
            wasm_path: self.wasm_path,
            enable_hydration: self.enable_hydration,
            signals: self.signals,
            command_handler: self.command_handler,
            allowed_actions: self.allowed_actions,
            rate_limit: self.rate_limit,
            pwa: self.pwa,
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Query parameters accepted by every route.
#[derive(serde::Deserialize, Default)]
struct ClientQuery {
    client: Option<String>,
}

/// Start the Axum server. Blocks forever.
pub async fn serve<Msg>(router: SmartRouter<Msg>, addr: SocketAddr)
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    let app = build_router(router);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind address");
    axum::serve(listener, app).await.expect("server error");
}

/// Build the Axum [`Router`] without starting it (useful for testing or
/// stacking with other middleware).
pub fn build_router<Msg>(router: SmartRouter<Msg>) -> Router
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    let serve_dir = ServeDir::new(&router.static_dir);
    let rate_limit = router.rate_limit;
    let state = std::sync::Arc::new(router);

    // Single shared bucket for the whole route rather than per-client: this
    // router doesn't plumb `ConnectInfo<SocketAddr>` (no
    // `into_make_service_with_connect_info` wiring), so `tower_governor`'s
    // per-IP key extractors aren't usable yet. Coarser than per-IP, but still
    // stops a single misbehaving/compromised client from hammering the one
    // write route this server exposes. Per-IP limiting is a documented
    // follow-up (todo.md).
    let governor_conf = std::sync::Arc::new(
        GovernorConfigBuilder::default()
            .per_second(rate_limit.per_second)
            .burst_size(rate_limit.burst)
            .key_extractor(GlobalKeyExtractor)
            .finish()
            .expect("valid governor rate-limit config"),
    );

    // Baseline security headers applied to every response — explicit rather
    // than absent. `X-Content-Type-Options` stops MIME-sniffing,
    // `X-Frame-Options` blocks clickjacking via iframes, and a conservative
    // default CSP restricts loaded resources to same-origin. Apps that need
    // a looser policy (e.g. embedding third-party fonts) can layer their own
    // `SetResponseHeaderLayer` on top of `build_router`'s output.
    Router::new()
        .route("/", get(root_handler::<Msg>))
        .route("/ai-schema.json", get(ai_schema_handler::<Msg>))
        .route("/opengraph", get(opengraph_handler::<Msg>))
        .route("/service-worker.js", get(pwa_service_worker::<Msg>))
        .route("/manifest.webmanifest", get(pwa_manifest::<Msg>))
        .route(
            "/command",
            post(command_handler::<Msg>)
                .route_layer(DefaultBodyLimit::max(COMMAND_BODY_LIMIT_BYTES))
                .route_layer(GovernorLayer { config: governor_conf }),
        )
        .fallback_service(serve_dir)
        .with_state(state)
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static("default-src 'self'"),
        ))
        .layer(TimeoutLayer::new(Duration::from_secs(10)))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

async fn root_handler<Msg>(
    state: axum::extract::State<std::sync::Arc<SmartRouter<Msg>>>,
    headers: HeaderMap,
    Query(query): Query<ClientQuery>,
) -> Response
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok());
    let kind = client_kind::detect(ua, query.client.as_deref());

    match kind {
        ClientKind::Human => human_shell(&state).await.into_response(),
        ClientKind::Crawler => crawler_html(&state).await.into_response(),
        ClientKind::AiAgent => ai_agent_json(&state).await.into_response(),
        ClientKind::SocialBot => social_opengraph(&state).await.into_response(),
    }
}

async fn ai_schema_handler<Msg>(
    state: axum::extract::State<std::sync::Arc<SmartRouter<Msg>>>,
) -> Response
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    ai_agent_json(&state).await.into_response()
}

async fn command_handler<Msg>(
    state: axum::extract::State<std::sync::Arc<SmartRouter<Msg>>>,
    Json(command): Json<Command>,
) -> Response {
    if command.action.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(CommandResponse::err("`action` must not be empty")),
        )
            .into_response();
    }

    if let Some(allowed) = &state.allowed_actions {
        if !allowed.iter().any(|a| a == &command.action) {
            return (
                StatusCode::FORBIDDEN,
                Json(CommandResponse::err(format!(
                    "action `{}` is not in the configured allowlist",
                    command.action
                ))),
            )
                .into_response();
        }
    }

    match &state.command_handler {
        Some(handler) => Json(handler(command)).into_response(),
        None => (
            StatusCode::NOT_IMPLEMENTED,
            Json(CommandResponse::err(
                "this router has no `on_command` handler configured",
            )),
        )
            .into_response(),
    }
}

async fn opengraph_handler<Msg>(
    state: axum::extract::State<std::sync::Arc<SmartRouter<Msg>>>,
) -> Response
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    social_opengraph(&state).await.into_response()
}

async fn pwa_service_worker<Msg>(
    state: axum::extract::State<std::sync::Arc<SmartRouter<Msg>>>,
) -> Response
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    match &state.pwa {
        Some(cfg) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/javascript")],
            service_worker(cfg),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn pwa_manifest<Msg>(
    state: axum::extract::State<std::sync::Arc<SmartRouter<Msg>>>,
) -> Response
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    match &state.pwa {
        Some(cfg) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/manifest+json")],
            manifest(cfg),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn human_shell<Msg>(state: &std::sync::Arc<SmartRouter<Msg>>) -> Html<String>
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    if !state.enable_hydration {
        // Legacy bare-WASM shell.
        let shell = state
            .wasm_shell_template
            .replace("{TITLE}", &appfront_html::esc_attr(&state.title))
            .replace("{DESC}", &appfront_html::esc_attr(&state.description))
            .replace("{WASM}", &state.wasm_path);
        return Html(inject_pwa(shell, state));
    }

    // Hydration page: SSR HTML + serialised state + WASM bootstrap.
    let mut ui = state.ui.clone();
    ui.assign_ids();

    let body = appfront_html::render(&ui);
    let payload = HydrationPayload {
        tree: ui,
        signals: state.signals.clone(),
    };
    let state_json = serde_json::to_string(&payload).unwrap_or_default();
    let wasm_path = &state.wasm_path;

    let page = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta property="og:title" content="{title}">
<meta property="og:description" content="{desc}">
<meta property="og:type" content="website">
</head>
<body>
<div id="appfront-root">
{body}
</div>
<script id="__APPFRONT_STATE__" type="application/json">{state_json}</script>
<script type="module">
import init from '{wasm_path}';
init().catch(e => console.error('appfront init failed', e));
</script>
</body>
</html>
"#,
        title = appfront_html::esc_attr(&state.title),
        desc = appfront_html::esc_attr(&state.description),
        body = body,
        state_json = appfront_html::esc_script_json(&state_json),
        wasm_path = wasm_path,
    );

    Html(inject_pwa(page, state))
}

/// Injects the PWA manifest `<link>` + service-worker registration `<script>`
/// into an HTML shell when [`SmartRouter::pwa`] is configured; returns the
/// input unchanged otherwise. The replacements are idempotent for the shell
/// shapes produced above.
fn inject_pwa<Msg>(mut page: String, state: &std::sync::Arc<SmartRouter<Msg>>) -> String {
    if state.pwa.is_none() {
        return page;
    }
    if let Some(head_end) = page.find("</head>") {
        page.insert_str(
            head_end,
            &format!("\n    {}", manifest_link()),
        );
    }
    if let Some(body_end) = page.rfind("</body>") {
        page.insert_str(
            body_end,
            &format!("\n    {}", registration_script()),
        );
    }
    page
}

async fn crawler_html<Msg>(state: &std::sync::Arc<SmartRouter<Msg>>) -> Html<String> {
    let page = appfront_html::render_page(&state.ui, &state.title, &state.description);
    Html(page)
}

async fn ai_agent_json<Msg>(state: &std::sync::Arc<SmartRouter<Msg>>) -> Json<serde_json::Value>
where
    Msg: serde::Serialize,
{
    let (json_ld, ai_schema) = appfront_ai_schema::both(&state.ui);
    let body = serde_json::json!({
        "jsonld": json_ld,
        "ai_schema": ai_schema,
    });
    Json(body)
}

async fn social_opengraph<Msg>(state: &std::sync::Arc<SmartRouter<Msg>>) -> Html<String> {
    let page = appfront_html::render_page(&state.ui, &state.title, &state.description);
    Html(page)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use appfront_core::ContainerBuilder;

    type Msg = ();

    fn test_router() -> SmartRouter<Msg> {
        let ui = UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Hello").class("title");
            c.button("Click").ai_action("greet");
        });
        SmartRouterBuilder::new(ui)
            .title("Test App")
            .description("A test app for the smart router")
            .static_dir("dist")
            .build()
    }

    #[tokio::test]
    async fn human_gets_shell() {
        let state = std::sync::Arc::new(test_router());
        let resp = human_shell(&state).await;
        assert!(resp.0.contains("<title>Test App</title>"));
        assert!(resp.0.contains("import init from '/app.wasm'"));
        assert!(!resp.0.contains("data-appfront-id"));
    }

    #[tokio::test]
    async fn legacy_shell_escapes_title_and_description() {
        let ui = UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Hello");
        });
        let router = SmartRouterBuilder::new(ui)
            .title("</title><script>alert(1)</script>")
            .description("<img src=x onerror=alert(1)>")
            .static_dir("dist")
            .build();
        let state = std::sync::Arc::new(router);

        let resp = human_shell(&state).await;
        assert!(!resp.0.contains("<script>alert(1)</script>"));
        assert!(!resp.0.contains("<img src=x onerror=alert(1)>"));
    }

    #[tokio::test]
    async fn hydration_page_escapes_title_description_and_state() {
        let mut signals = HashMap::new();
        signals.insert(
            "evil".to_string(),
            serde_json::json!("</script><script>alert(1)</script>"),
        );

        let ui = UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Hello");
        });
        let router = SmartRouterBuilder::new(ui)
            .title("</title><script>alert(2)</script>")
            .description("<img src=x onerror=alert(2)>")
            .enable_hydration(true)
            .signals(signals)
            .build();
        let state = std::sync::Arc::new(router);

        let resp = human_shell(&state).await;
        let html = &resp.0;

        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(!html.contains("<script>alert(2)</script>"));
        assert!(!html.contains("<img src=x onerror=alert(2)>"));
        // The script tag housing the state must not be terminated early.
        assert!(!html.contains("</script><script>alert(1)</script>"));
    }

    #[tokio::test]
    async fn human_gets_hydration_page_when_enabled() {
        let mut signals = HashMap::new();
        signals.insert("count".to_string(), serde_json::json!(42));

        let ui = UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Hello").class("title");
            c.button("Click").ai_action("greet");
        });
        let router = SmartRouterBuilder::new(ui)
            .title("Test App")
            .description("A test app")
            .enable_hydration(true)
            .signals(signals)
            .build();
        let state = std::sync::Arc::new(router);

        let resp = human_shell(&state).await;
        let html = &resp.0;

        // Should contain SSR content
        assert!(html.contains("<h1 class=\"title\" data-appfront-id=\"2\">Hello</h1>"));

        // Should contain the serialised state script
        assert!(html.contains("__APPFRONT_STATE__"));
        assert!(html.contains(r#""signals":{"count":42}"#));

        // Should contain the WASM script
        assert!(html.contains("import init from '/app.wasm'"));
    }

    #[tokio::test]
    async fn crawler_gets_html() {
        let state = std::sync::Arc::new(test_router());
        let resp = crawler_html(&state).await;
        assert!(resp.0.contains("<!DOCTYPE html>"));
        assert!(resp.0.contains("<h1 class=\"title\">Hello</h1>"));
    }

    #[tokio::test]
    async fn ai_gets_json() {
        let state = std::sync::Arc::new(test_router());
        let resp = ai_agent_json(&state).await;
        assert!(resp.0.get("jsonld").is_some());
        assert!(resp.0.get("ai_schema").is_some());
    }

    #[tokio::test]
    async fn social_gets_html() {
        let state = std::sync::Arc::new(test_router());
        let resp = social_opengraph(&state).await;
        assert!(resp.0.contains("og:title"));
    }

    #[tokio::test]
    async fn command_without_handler_returns_501() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::util::ServiceExt;

        let app = build_router(test_router());
        let req = Request::builder()
            .method("POST")
            .uri("/command")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"action":"greet"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn command_rejects_empty_action() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::util::ServiceExt;

        let router = SmartRouterBuilder::new(UITree::<Msg>::container(|_| {}))
            .on_command(|_cmd| CommandResponse::ok("should not run"))
            .build();
        let app = build_router(router);
        let req = Request::builder()
            .method("POST")
            .uri("/command")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"action":""}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn command_rejects_action_outside_allowlist() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::util::ServiceExt;

        let router = SmartRouterBuilder::new(UITree::<Msg>::container(|_| {}))
            .on_command(|_cmd| CommandResponse::ok("should not run"))
            .allowed_actions(["greet"])
            .build();
        let app = build_router(router);
        let req = Request::builder()
            .method("POST")
            .uri("/command")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"action":"delete_everything"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn command_invokes_handler_and_returns_response() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::util::ServiceExt;

        let router = SmartRouterBuilder::new(UITree::<Msg>::container(|_| {}))
            .on_command(|cmd| {
                assert_eq!(cmd.action, "greet");
                assert_eq!(
                    cmd.params.get("name").and_then(|v| v.as_str()),
                    Some("Ada")
                );
                CommandResponse::ok("greeted Ada")
            })
            .allowed_actions(["greet"])
            .build();
        let app = build_router(router);
        let req = Request::builder()
            .method("POST")
            .uri("/command")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"action":"greet","params":{"name":"Ada"}}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["message"], "greeted Ada");
    }

    #[tokio::test]
    async fn command_rejects_oversized_body() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::util::ServiceExt;

        let router = SmartRouterBuilder::new(UITree::<Msg>::container(|_| {}))
            .on_command(|_cmd| CommandResponse::ok("should not run"))
            .build();
        let app = build_router(router);

        let oversized = format!(
            r#"{{"action":"{}"}}"#,
            "a".repeat(COMMAND_BODY_LIMIT_BYTES + 1)
        );
        let req = Request::builder()
            .method("POST")
            .uri("/command")
            .header("content-type", "application/json")
            .body(Body::from(oversized))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn command_rejects_requests_beyond_the_rate_limit() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::util::ServiceExt;

        let router = SmartRouterBuilder::new(UITree::<Msg>::container(|_| {}))
            .on_command(|_cmd| CommandResponse::ok("ok"))
            .rate_limit(RateLimitConfig { per_second: 1, burst: 3 })
            .build();
        let app = build_router(router);

        let make_req = || {
            Request::builder()
                .method("POST")
                .uri("/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"greet"}"#))
                .unwrap()
        };

        for _ in 0..3 {
            let resp = app.clone().oneshot(make_req()).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }

        // The burst allowance (3) is now exhausted; the next request should
        // be throttled.
        let resp = app.clone().oneshot(make_req()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn pwa_routes_served_and_shell_injects_glue() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::util::ServiceExt;

        let router = SmartRouterBuilder::new(UITree::<Msg>::container(|_| {}))
            .title("PWA App")
            .pwa(PwaConfig {
                precache: vec!["/".to_string(), "/app.wasm".to_string()],
                ..Default::default()
            })
            .build();
        let app = build_router(router);

        let sw = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/service-worker.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(sw.status(), StatusCode::OK);
        assert_eq!(
            sw.headers().get(axum::http::header::CONTENT_TYPE).unwrap(),
            "application/javascript"
        );

        let m = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/manifest.webmanifest")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(m.status(), StatusCode::OK);

        // No PWA config -> 404 instead of serving the assets.
        let app2 = build_router(test_router());
        let sw2 = app2
            .oneshot(
                Request::builder()
                    .uri("/service-worker.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(sw2.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn hydration_shell_injects_pwa_glue() {
        let router = SmartRouterBuilder::new(UITree::<Msg>::container(|c| {
            c.heading(1, "Hello");
        }))
        .title("PWA App")
        .enable_hydration(true)
        .pwa(PwaConfig::default())
        .build();
        let state = std::sync::Arc::new(router);
        let resp = human_shell(&state).await;
        let html = &resp.0;
        assert!(html.contains(r#"rel="manifest""#));
        assert!(html.contains("/manifest.webmanifest"));
        assert!(html.contains("/service-worker.js"));
        assert!(html.contains("navigator.serviceWorker.register"));
    }

    #[tokio::test]
    async fn root_routes_by_ua() {
        let router = test_router();
        use axum::http::Request;
        use axum::http::StatusCode;
        use tower::util::ServiceExt;

        let app = build_router(router);

        let req = Request::builder()
            .uri("/")
            .header("user-agent", "Mozilla/5.0 Chrome/120")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let req = Request::builder()
            .uri("/")
            .header("user-agent", "Googlebot/2.1")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let req = Request::builder()
            .uri("/")
            .header("user-agent", "GPTBot/1.0")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
