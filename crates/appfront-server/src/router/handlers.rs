//! Axum handlers for the smart router's routes.

use std::sync::Arc;

use appfront_core::HydrationPayload;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Json, Response};

use crate::client_kind::{self, ClientKind};
use crate::pwa::{manifest, manifest_link, registration_script, service_worker};
use crate::router::command::{Command, CommandResponse};
use crate::router::SmartRouter;

/// Query parameters accepted by every route.
#[derive(serde::Deserialize, Default)]
pub(crate) struct ClientQuery {
    client: Option<String>,
}

/// Generates a per-response, unique nonce for the document CSP. Combines a
/// monotonic counter with the current time so each response gets a distinct
/// value (good enough to bind inline scripts to their document) without
/// pulling in a crypto RNG dependency.
fn next_nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:016x}{c:016x}")
}

/// Attaches a per-response `Content-Security-Policy` header (carrying the
/// document nonce) to an `Html` response. The strict CSP is set per-document
/// rather than globally so the inline WASM bootstrap / PWA registration
/// scripts can be allow-listed by their nonce instead of being blocked.
fn csp_response(mut resp: Response, csp: &str) -> Response {
    if let Ok(v) = HeaderValue::from_str(csp) {
        resp.headers_mut()
            .insert(axum::http::header::CONTENT_SECURITY_POLICY, v);
    }
    resp
}

pub(crate) async fn root_handler<Msg>(
    state: State<Arc<SmartRouter<Msg>>>,
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

pub(crate) async fn ai_schema_handler<Msg>(
    state: State<Arc<SmartRouter<Msg>>>,
) -> Response
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    ai_agent_json(&state).await.into_response()
}

pub(crate) async fn command_handler<Msg>(
    state: State<Arc<SmartRouter<Msg>>>,
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

pub(crate) async fn opengraph_handler<Msg>(
    state: State<Arc<SmartRouter<Msg>>>,
) -> Response
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    social_opengraph(&state).await.into_response()
}

pub(crate) async fn pwa_service_worker<Msg>(
    state: State<Arc<SmartRouter<Msg>>>,
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

pub(crate) async fn pwa_manifest<Msg>(
    state: State<Arc<SmartRouter<Msg>>>,
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

pub(crate) async fn human_shell<Msg>(state: &Arc<SmartRouter<Msg>>) -> Response
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    // Fresh nonce per response; the same value is threaded into the inline
    // scripts and the document CSP so the app's own bootstrap/registration
    // scripts are allow-listed while everything else stays same-origin.
    let nonce = next_nonce();
    let csp = format!(
        "script-src 'self' 'nonce-{nonce}' 'wasm-unsafe-eval'; object-src 'none'; base-uri 'self'"
    );

    if !state.enable_hydration {
        // Legacy bare-WASM shell.
        let shell = state
            .wasm_shell_template
            .replace("{TITLE}", &appfront_html::esc_attr(&state.title))
            .replace("{DESC}", &appfront_html::esc_attr(&state.description))
            .replace("{WASM}", &appfront_html::esc_attr(&state.wasm_path))
            .replace(
                "<script type=\"module\">",
                &format!("<script type=\"module\" nonce=\"{nonce}\">"),
            );
        return csp_response(Html(inject_pwa(shell, state, &nonce)).into_response(), &csp);
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
<script type="module" nonce="{nonce}">
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
        wasm_path = appfront_html::esc_attr(&state.wasm_path),
        nonce = nonce,
    );

    csp_response(Html(inject_pwa(page, state, &nonce)).into_response(), &csp)
}

/// Injects the PWA manifest `<link>` + service-worker registration `<script>`
/// into an HTML shell when [`SmartRouter::pwa`] is configured; returns the
/// input unchanged otherwise. The replacements are idempotent for the shell
/// shapes produced above. `nonce` is forwarded to the registration script so
/// its inline `<script>` satisfies the document CSP.
fn inject_pwa<Msg>(
    mut page: String,
    state: &Arc<SmartRouter<Msg>>,
    nonce: &str,
) -> String {
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
            &format!("\n    {}", registration_script(nonce)),
        );
    }
    page
}

pub(crate) async fn crawler_html<Msg>(state: &Arc<SmartRouter<Msg>>) -> Html<String> {
    let page = appfront_html::render_page(&state.ui, &state.title, &state.description);
    Html(page)
}

pub(crate) async fn ai_agent_json<Msg>(state: &Arc<SmartRouter<Msg>>) -> Json<serde_json::Value>
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

pub(crate) async fn social_opengraph<Msg>(state: &Arc<SmartRouter<Msg>>) -> Html<String> {
    let page = appfront_html::render_page(&state.ui, &state.title, &state.description);
    Html(page)
}
