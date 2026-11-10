//! Handlers for the smart router's routes. Each takes the shared router state and
//! returns a finished [`Resp`]. Request parsing happens in `crate::app`.

use std::sync::Arc;

use http::{header, HeaderMap, HeaderValue, StatusCode};
use tpt_appfront_core::HydrationPayload;

use crate::app::{bare, html, json, query_param, with_type, Resp};
use crate::client_kind::{self, ClientKind};
use crate::pwa::{
    manifest, manifest_link, registration_script, service_worker, update_available_script,
};
use crate::router::command::{Command, CommandResponse};
use crate::router::{caching, csrf, SmartRouter};

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
/// document nonce) to an HTML response. The strict CSP is set per-document
/// rather than globally so the inline WASM bootstrap / PWA registration
/// scripts can be allow-listed by their nonce instead of being blocked.
///
/// When `csrf` is set, also attaches a fresh CSRF cookie: this is the only
/// document route a browser client fetches before it would ever call
/// `POST /command`, so it's where the double-submit token gets minted (see
/// `crate::router::csrf`).
fn csp_response(mut resp: Resp, csp: &str, csrf: bool) -> Resp {
    if let Ok(v) = HeaderValue::from_str(csp) {
        resp.headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, v);
    }
    if csrf {
        let token = csrf::generate_token();
        resp.headers_mut()
            .insert(header::SET_COOKIE, csrf::set_cookie_header(&token));
    }
    resp
}

pub(crate) async fn root_response<Msg>(
    state: &Arc<SmartRouter<Msg>>,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Resp
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok());
    let extra_ai = state
        .extra_ai_agents
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>();
    let client = query_param(query, "client");
    let kind = client_kind::detect_with(ua, client.as_deref(), &extra_ai);

    match kind {
        ClientKind::Human => human_shell(state).await,
        ClientKind::Crawler => crawler_html(state, headers).await,
        ClientKind::AiAgent => ai_agent_json(state, headers).await,
        ClientKind::SocialBot => social_opengraph(state, headers).await,
    }
}

pub(crate) async fn ai_schema_response<Msg>(
    state: &Arc<SmartRouter<Msg>>,
    headers: &HeaderMap,
) -> Resp
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    ai_agent_json(state, headers).await
}

pub(crate) async fn opengraph_response<Msg>(
    state: &Arc<SmartRouter<Msg>>,
    headers: &HeaderMap,
) -> Resp
where
    Msg: Clone + Send + Sync + serde::Serialize + 'static,
{
    social_opengraph(state, headers).await
}

/// `POST /command` after the transport checks (rate limit, body size, JSON).
/// Applies the CSRF, auth-hook, action-allowlist and handler steps in that order.
pub(crate) fn command_response<Msg>(
    state: &Arc<SmartRouter<Msg>>,
    headers: &HeaderMap,
    command: Command,
) -> Resp {
    if state.csrf && !csrf::verify(headers) {
        return json(
            StatusCode::FORBIDDEN,
            &CommandResponse::err("missing or invalid CSRF token"),
        );
    }

    if let Some(hook) = &state.auth_hook {
        if !hook(headers) {
            return json(
                StatusCode::UNAUTHORIZED,
                &CommandResponse::err("unauthorized"),
            );
        }
    }

    if command.action.trim().is_empty() {
        return json(
            StatusCode::BAD_REQUEST,
            &CommandResponse::err("`action` must not be empty"),
        );
    }

    if let Some(allowed) = &state.allowed_actions {
        if !allowed.iter().any(|a| a == &command.action) {
            return json(
                StatusCode::FORBIDDEN,
                &CommandResponse::err(format!(
                    "action `{}` is not in the configured allowlist",
                    command.action
                )),
            );
        }
    }

    match &state.command_handler {
        Some(handler) => json(StatusCode::OK, &handler(command)),
        None => json(
            StatusCode::NOT_IMPLEMENTED,
            &CommandResponse::err("this router has no `on_command` handler configured"),
        ),
    }
}

pub(crate) fn service_worker_response<Msg>(state: &Arc<SmartRouter<Msg>>) -> Resp {
    match &state.pwa {
        Some(cfg) => with_type(
            StatusCode::OK,
            "application/javascript",
            service_worker(cfg),
        ),
        None => bare(StatusCode::NOT_FOUND),
    }
}

pub(crate) fn manifest_response<Msg>(state: &Arc<SmartRouter<Msg>>) -> Resp {
    match &state.pwa {
        Some(cfg) => with_type(StatusCode::OK, "application/manifest+json", manifest(cfg)),
        None => bare(StatusCode::NOT_FOUND),
    }
}

pub(crate) async fn human_shell<Msg>(state: &Arc<SmartRouter<Msg>>) -> Resp
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
            .replace("{TITLE}", &tpt_appfront_html::esc_attr(&state.title))
            .replace("{DESC}", &tpt_appfront_html::esc_attr(&state.description))
            .replace("{WASM}", &tpt_appfront_html::esc_attr(&state.wasm_path))
            .replace(
                "<script type=\"module\">",
                &format!("<script type=\"module\" nonce=\"{nonce}\">"),
            );
        return csp_response(html(inject_pwa(shell, state, &nonce)), &csp, state.csrf);
    }

    // Hydration page: SSR HTML + serialised state + WASM bootstrap.
    let mut ui = state.ui.clone();
    ui.assign_ids();

    let body = tpt_appfront_html::render(&ui);
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
        title = tpt_appfront_html::esc_attr(&state.title),
        desc = tpt_appfront_html::esc_attr(&state.description),
        body = body,
        state_json = tpt_appfront_html::esc_script_json(&state_json),
        wasm_path = tpt_appfront_html::esc_attr(&state.wasm_path),
        nonce = nonce,
    );

    csp_response(html(inject_pwa(page, state, &nonce)), &csp, state.csrf)
}

/// Injects the PWA manifest `<link>` + service-worker registration `<script>`
/// into an HTML shell when [`SmartRouter::pwa`] is configured; returns the
/// input unchanged otherwise. The replacements are idempotent for the shell
/// shapes produced above. `nonce` is forwarded to the registration script so
/// its inline `<script>` satisfies the document CSP.
fn inject_pwa<Msg>(mut page: String, state: &Arc<SmartRouter<Msg>>, nonce: &str) -> String {
    let Some(cfg) = state.pwa.as_ref() else {
        return page;
    };
    if let Some(head_end) = page.find("</head>") {
        page.insert_str(head_end, &format!("\n    {}", manifest_link()));
    }
    if let Some(body_end) = page.rfind("</body>") {
        page.insert_str(
            body_end,
            &format!(
                "\n    {}\n    {}",
                registration_script(cfg, nonce),
                update_available_script(nonce),
            ),
        );
    }
    page
}

pub(crate) async fn crawler_html<Msg>(state: &Arc<SmartRouter<Msg>>, headers: &HeaderMap) -> Resp {
    caching::cached_html(&state.html_cache, headers, || {
        tpt_appfront_html::render_page(&state.ui, &state.title, &state.description)
    })
}

pub(crate) async fn ai_agent_json<Msg>(state: &Arc<SmartRouter<Msg>>, headers: &HeaderMap) -> Resp
where
    Msg: serde::Serialize,
{
    caching::cached_json(&state.ai_schema_cache, headers, || {
        let (json_ld, ai_schema) = tpt_appfront_ai_schema::both(&state.ui);
        serde_json::json!({
            "jsonld": json_ld,
            "ai_schema": ai_schema,
        })
    })
}

pub(crate) async fn social_opengraph<Msg>(
    state: &Arc<SmartRouter<Msg>>,
    headers: &HeaderMap,
) -> Resp {
    caching::cached_html(&state.opengraph_cache, headers, || {
        tpt_appfront_html::render_page(&state.ui, &state.title, &state.description)
    })
}
