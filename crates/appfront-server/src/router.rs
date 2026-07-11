//! Axum-based smart router that detects client type and serves the
//! appropriate rendering backend.

mod command;
mod cors;
mod handlers;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use appfront_core::UITree;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, HeaderValue};
use axum::routing::{get, post};
use axum::Router;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::PeerIpKeyExtractor;
use tower_governor::GovernorLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::pwa::PwaConfig;

// Re-export the public command/CORS types so the crate's documented API and
// the inline `mod tests` (which `use super::*`) resolve them unchanged.
pub use command::{Command, CommandHandler, CommandResponse, RateLimitConfig};
pub use cors::CorsPolicy;

/// Maximum accepted body size for `POST /command`, applied before the
/// request reaches the handler — closes the gap noted in Phase 12: no
/// write route should ship without a body-size limit from day one.
const COMMAND_BODY_LIMIT_BYTES: usize = 16 * 1024;



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
    /// Cross-origin policy for the read routes. Defaults to
    /// [`CorsPolicy::Permissive`] (unchanged from prior behavior); override
    /// via [`SmartRouterBuilder::cors`] to restrict to an origin allowlist.
    pub cors: CorsPolicy,
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
    cors: CorsPolicy,
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
            cors: CorsPolicy::default(),
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

    /// Override the default [`CorsPolicy::Permissive`] applied to the read
    /// routes. Use [`CorsPolicy::Origins`] to restrict cross-origin reads to
    /// a specific allowlist.
    pub fn cors(mut self, policy: CorsPolicy) -> Self {
        self.cors = policy;
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
            cors: self.cors,
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Start the Axum server. Blocks forever.
///
/// Uses [`Router::into_make_service_with_connect_info`] so the per-client
/// (peer IP) rate limiter on `POST /command` has a real address to key on;
/// see [`build_router`]'s docs if you're wiring your own `axum::serve` call
/// instead of using this function.
pub async fn serve<Msg>(router: SmartRouter<Msg>, addr: SocketAddr)
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    let app = build_router(router);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind address");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server error");
}

/// Build the Axum [`Router`] without starting it (useful for testing or
/// stacking with other middleware).
///
/// `POST /command` is rate-limited per peer IP ([`PeerIpKeyExtractor`]),
/// which requires an `axum::extract::ConnectInfo<SocketAddr>` request
/// extension to be present. If you serve this router yourself instead of
/// calling [`serve`], make sure to do so via
/// `.into_make_service_with_connect_info::<SocketAddr>()` — otherwise every
/// request to `/command` will fail key extraction.
pub fn build_router<Msg>(router: SmartRouter<Msg>) -> Router
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    let serve_dir = ServeDir::new(&router.static_dir);
    let rate_limit = router.rate_limit;
    let cors = router.cors.clone();
    let state = std::sync::Arc::new(router);

    // Per-peer-IP bucket rather than one shared bucket for the whole route,
    // so a single misbehaving/compromised client is throttled without
    // affecting other clients. Note the `PeerIpKeyExtractor` caveat: behind a
    // reverse proxy the peer IP is the proxy's IP unless the proxy forwards
    // the real client IP and the app switches to `SmartIpKeyExtractor` (which
    // trusts `X-Forwarded-For`/`X-Real-Ip`/`Forwarded` headers — only safe if
    // those headers are guaranteed to come from a trusted proxy).
    let governor_conf = std::sync::Arc::new(
        GovernorConfigBuilder::default()
            .per_second(rate_limit.per_second)
            .burst_size(rate_limit.burst)
            .key_extractor(PeerIpKeyExtractor)
            .finish()
            .expect("valid governor rate-limit config"),
    );

    // Read/asset routes may be fetched cross-origin by browsers and AI
    // agents; the exact policy is configurable via
    // `SmartRouterBuilder::cors` (defaults to permissive, unchanged from
    // prior behavior). The state-changing `POST /command` (built separately
    // below) is deliberately left without CORS so it can't be driven
    // cross-site.
    let read_routes = Router::new()
        .route("/", get(handlers::root_handler::<Msg>))
        .route("/ai-schema.json", get(handlers::ai_schema_handler::<Msg>))
        .route("/opengraph", get(handlers::opengraph_handler::<Msg>))
        .route("/service-worker.js", get(handlers::pwa_service_worker::<Msg>))
        .route("/manifest.webmanifest", get(handlers::pwa_manifest::<Msg>))
        .layer(cors::cors_layer(&cors));

    // The single write route: body-size-limited and rate-limited, and NOT
    // CORS-exposed, so a third-party page can't invoke app commands for the
    // victim (CSRF-style).
    let command_routes = Router::new().route(
        "/command",
        post(handlers::command_handler::<Msg>)
            .route_layer(DefaultBodyLimit::max(COMMAND_BODY_LIMIT_BYTES))
            .route_layer(GovernorLayer { config: governor_conf }),
    );

    // Baseline security headers applied to every response — explicit rather
    // than absent. `X-Content-Type-Options` stops MIME-sniffing and
    // `X-Frame-Options` blocks clickjacking. The strict `Content-Security-Policy`
    // is set *per document* by `human_shell` (with a fresh nonce) so the app's
    // own inline WASM bootstrap / PWA registration scripts are allow-listed
    // while everything else stays same-origin. Apps needing a looser policy
    // (e.g. third-party fonts) can layer their own `SetResponseHeaderLayer`
    // on top of `build_router`'s output.
    read_routes
        .merge(command_routes)
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
        .layer(TimeoutLayer::new(Duration::from_secs(10)))
        .layer(TraceLayer::new_for_http())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use appfront_core::ContainerBuilder;
    use axum::response::Response;
    use crate::router::handlers::{ai_agent_json, crawler_html, human_shell, social_opengraph};

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

    /// Extracts the HTML body from a `human_shell` response (now a full
    /// `Response`, since the document CSP header is set on it).
    async fn shell_html(state: &std::sync::Arc<SmartRouter<Msg>>) -> String {
        body_string(human_shell(state).await).await
    }

    async fn body_string(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("response body");
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// A `ConnectInfo` extension standing in for the peer address axum would
    /// normally attach via `into_make_service_with_connect_info` — required
    /// for `PeerIpKeyExtractor` to find a rate-limit key in `oneshot` tests.
    fn test_connect_info() -> axum::extract::ConnectInfo<SocketAddr> {
        connect_info_for([127, 0, 0, 1])
    }

    fn connect_info_for(ip: [u8; 4]) -> axum::extract::ConnectInfo<SocketAddr> {
        axum::extract::ConnectInfo(SocketAddr::from((ip, 0)))
    }

    #[tokio::test]
    async fn human_gets_shell() {
        let state = std::sync::Arc::new(test_router());
        let resp = human_shell(&state).await;
        let csp = resp
            .headers()
            .get(axum::http::header::CONTENT_SECURITY_POLICY)
            .expect("CSP header present")
            .to_str()
            .unwrap();
        assert!(csp.contains("nonce-"));
        assert!(csp.contains("script-src 'self'"));
        let html = body_string(resp).await;
        assert!(html.contains("<title>Test App</title>"));
        assert!(html.contains("import init from '/app.wasm'"));
        assert!(!html.contains("data-appfront-id"));
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

        let html = shell_html(&state).await;
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(!html.contains("<img src=x onerror=alert(1)>"));
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

        let html = shell_html(&state).await;

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

        let html = shell_html(&state).await;

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
            .extension(test_connect_info())
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
            .extension(test_connect_info())
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
            .extension(test_connect_info())
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
            .extension(test_connect_info())
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
            .extension(test_connect_info())
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
                .extension(test_connect_info())
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
    async fn command_rate_limit_is_per_peer_ip() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::util::ServiceExt;

        let router = SmartRouterBuilder::new(UITree::<Msg>::container(|_| {}))
            .on_command(|_cmd| CommandResponse::ok("ok"))
            .rate_limit(RateLimitConfig { per_second: 1, burst: 1 })
            .build();
        let app = build_router(router);

        let make_req = |ip: [u8; 4]| {
            Request::builder()
                .method("POST")
                .uri("/command")
                .header("content-type", "application/json")
                .extension(connect_info_for(ip))
                .body(Body::from(r#"{"action":"greet"}"#))
                .unwrap()
        };

        // Client A exhausts its single-request burst allowance.
        let resp = app.clone().oneshot(make_req([10, 0, 0, 1])).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let resp = app.clone().oneshot(make_req([10, 0, 0, 1])).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        // Client B has its own bucket and is unaffected by A's usage.
        let resp = app.clone().oneshot(make_req([10, 0, 0, 2])).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
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
        let html = shell_html(&state).await;
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

    #[tokio::test]
    async fn default_cors_is_permissive() {
        use axum::http::{Request, StatusCode};
        use tower::util::ServiceExt;

        let app = build_router(test_router());
        let req = Request::builder()
            .uri("/")
            .header("origin", "https://evil.example")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "*"
        );
    }

    #[tokio::test]
    async fn cors_origins_policy_restricts_allowed_origin_header() {
        use axum::http::{Request, StatusCode};
        use tower::util::ServiceExt;

        let router = SmartRouterBuilder::new(UITree::<Msg>::container(|_| {}))
            .cors(CorsPolicy::Origins(vec!["https://trusted.example".to_string()]))
            .build();
        let app = build_router(router);

        let allowed = Request::builder()
            .uri("/")
            .header("origin", "https://trusted.example")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(allowed).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "https://trusted.example"
        );

        let disallowed = Request::builder()
            .uri("/")
            .header("origin", "https://evil.example")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(disallowed).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp
            .headers()
            .get(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());
    }
}
