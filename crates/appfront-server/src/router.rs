//! Axum-based smart router that detects client type and serves the
//! appropriate rendering backend.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use appfront_core::{HydrationPayload, UITree};
use axum::extract::Query;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use tower_http::services::ServeDir;

use crate::client_kind::{self, ClientKind};

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

    pub fn build(self) -> SmartRouter<Msg> {
        let wasm_shell_template = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{0}</title>
<meta property="og:title" content="{0}">
<meta property="og:description" content="{1}">
<meta property="og:type" content="website">
</head>
<body>
<div id="appfront-root"></div>
<script type="module">
import init from '{2}';
init().catch(e => console.error('appfront init failed', e));
</script>
</body>
</html>
"#,
            "{}", "{}", "{}"
        );

        SmartRouter {
            ui: self.ui,
            title: self.title,
            description: self.description,
            static_dir: self.static_dir,
            wasm_shell_template,
            wasm_path: self.wasm_path,
            enable_hydration: self.enable_hydration,
            signals: self.signals,
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
    let state = std::sync::Arc::new(router);

    Router::new()
        .route("/", get(root_handler::<Msg>))
        .route("/ai-schema.json", get(ai_schema_handler::<Msg>))
        .route("/opengraph", get(opengraph_handler::<Msg>))
        .fallback_service(serve_dir)
        .with_state(state)
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

async fn opengraph_handler<Msg>(
    state: axum::extract::State<std::sync::Arc<SmartRouter<Msg>>>,
) -> Response
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    social_opengraph(&state).await.into_response()
}

async fn human_shell<Msg>(state: &std::sync::Arc<SmartRouter<Msg>>) -> Html<String>
where
    Msg: Clone + serde::Serialize + 'static,
{
    if !state.enable_hydration {
        // Legacy bare-WASM shell.
        let shell = state
            .wasm_shell_template
            .replacen("{}", &state.title, 1)
            .replacen("{}", &state.description, 1)
            .replacen("{}", &state.wasm_path, 1);
        return Html(shell);
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
        title = state.title,
        desc = state.description,
        body = body,
        state_json = state_json,
        wasm_path = wasm_path,
    );

    Html(page)
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
        assert!(!resp.0.contains("data-appfront-id"));
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
