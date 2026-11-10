//! The server's HTTP layer, built directly on hyper.
//!
//! This replaces axum and tower-http, which pulled in `sync_wrapper` (banned by
//! `deny.toml`). It provides what the smart router needs: method and path
//! routing, a request timeout, a body-size limit, security headers, CORS,
//! per-client rate limiting, static files from the configured directory, request
//! tracing, and the hyper accept loop.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::{header, HeaderMap, HeaderName, HeaderValue, Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full, Limited};
use serde::Serialize;

use crate::rate_limit::KeyedRateLimiter;
use crate::router::SmartRouter;

/// The response type every route produces.
pub(crate) type Resp = Response<Full<Bytes>>;

/// Requests that take longer than this get `408 Request Timeout`.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum accepted body size for `POST /command`.
pub(crate) const COMMAND_BODY_LIMIT_BYTES: usize = 16 * 1024;

const X_CONTENT_TYPE_OPTIONS: HeaderName = HeaderName::from_static("x-content-type-options");
const X_FRAME_OPTIONS: HeaderName = HeaderName::from_static("x-frame-options");

// ---------------------------------------------------------------------------
// Response helpers
// ---------------------------------------------------------------------------

/// An empty response with the given status.
pub(crate) fn bare(status: StatusCode) -> Resp {
    let mut resp = Response::new(Full::default());
    *resp.status_mut() = status;
    resp
}

/// A response with an explicit content type.
pub(crate) fn with_type(
    status: StatusCode,
    content_type: &'static str,
    body: impl Into<Bytes>,
) -> Resp {
    let mut resp = Response::new(Full::new(body.into()));
    *resp.status_mut() = status;
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    resp
}

/// A `200 OK` HTML response.
pub(crate) fn html(body: impl Into<Bytes>) -> Resp {
    with_type(StatusCode::OK, "text/html; charset=utf-8", body)
}

/// A JSON response with the given status.
pub(crate) fn json<T: Serialize>(status: StatusCode, value: &T) -> Resp {
    let body = serde_json::to_vec(value).unwrap_or_default();
    with_type(status, "application/json", body)
}

/// A `405 Method Not Allowed` response that lists the allowed methods.
fn method_not_allowed(allow: &'static str) -> Resp {
    let mut resp = bare(StatusCode::METHOD_NOT_ALLOWED);
    resp.headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static(allow));
    resp
}

// ---------------------------------------------------------------------------
// Routing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    Root,
    AiSchema,
    OpenGraph,
    ServiceWorker,
    Manifest,
    Command,
}

/// Maps an exact request path to a router route. Unknown paths fall through to
/// the static-file handler.
pub(crate) fn route_for(path: &str) -> Option<Route> {
    Some(match path {
        "/" => Route::Root,
        "/ai-schema.json" => Route::AiSchema,
        "/opengraph" => Route::OpenGraph,
        "/service-worker.js" => Route::ServiceWorker,
        "/manifest.webmanifest" => Route::Manifest,
        "/command" => Route::Command,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// The request handler
// ---------------------------------------------------------------------------

/// The router's request handler. Cheap to clone: the state and the rate limiters
/// are shared, so every clone sees the same buckets.
pub struct Router<Msg> {
    pub(crate) state: Arc<SmartRouter<Msg>>,
    pub(crate) read_limiter: Arc<KeyedRateLimiter>,
    pub(crate) command_limiter: Arc<KeyedRateLimiter>,
}

impl<Msg> Clone for Router<Msg> {
    fn clone(&self) -> Self {
        Router {
            state: self.state.clone(),
            read_limiter: self.read_limiter.clone(),
            command_limiter: self.command_limiter.clone(),
        }
    }
}

impl<Msg> Router<Msg>
where
    Msg: Clone + Send + Sync + Serialize + 'static,
{
    /// Handles one request. The peer address, if known, is read from a
    /// `SocketAddr` request extension, which [`crate::serve`] inserts for every
    /// connection. Without it, rate limiting needs `X-Forwarded-For` (when
    /// trusted) and otherwise answers `500`.
    pub async fn handle<B>(&self, req: Request<B>) -> Resp
    where
        B: http_body::Body<Data = Bytes> + Send,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let started = Instant::now();
        let method = req.method().clone();
        let path = req.uri().path().to_owned();

        let mut resp = match tokio::time::timeout(REQUEST_TIMEOUT, self.dispatch(req)).await {
            Ok(resp) => resp,
            Err(_) => bare(StatusCode::REQUEST_TIMEOUT),
        };

        let headers = resp.headers_mut();
        headers
            .entry(X_CONTENT_TYPE_OPTIONS)
            .or_insert(HeaderValue::from_static("nosniff"));
        headers
            .entry(X_FRAME_OPTIONS)
            .or_insert(HeaderValue::from_static("DENY"));

        tracing::info!(
            method = %method,
            path = %path,
            status = resp.status().as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "request"
        );
        resp
    }

    async fn dispatch<B>(&self, req: Request<B>) -> Resp
    where
        B: http_body::Body<Data = Bytes> + Send,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let peer = req.extensions().get::<SocketAddr>().map(SocketAddr::ip);
        let (parts, body) = req.into_parts();
        match route_for(parts.uri.path()) {
            Some(Route::Command) => {
                self.command(&parts.method, &parts.headers, peer, body)
                    .await
            }
            Some(route) => {
                self.read(
                    route,
                    &parts.method,
                    &parts.headers,
                    parts.uri.query(),
                    peer,
                )
                .await
            }
            None => self.static_file(&parts.method, parts.uri.path()).await,
        }
    }

    /// The read routes: `GET` (and `HEAD`), rate limited, with CORS applied.
    async fn read(
        &self,
        route: Route,
        method: &Method,
        headers: &HeaderMap,
        query: Option<&str>,
        peer: Option<std::net::IpAddr>,
    ) -> Resp {
        let cors = &self.state.cors;
        if *method == Method::OPTIONS {
            return crate::router::cors::preflight(cors, headers);
        }
        if *method != Method::GET && *method != Method::HEAD {
            return method_not_allowed("GET, HEAD, OPTIONS");
        }
        if let Err(resp) = self.read_limiter.check_request(headers, peer) {
            return *resp;
        }

        let mut resp = match route {
            Route::Root => {
                crate::router::handlers::root_response(&self.state, headers, query).await
            }
            Route::AiSchema => {
                crate::router::handlers::ai_schema_response(&self.state, headers).await
            }
            Route::OpenGraph => {
                crate::router::handlers::opengraph_response(&self.state, headers).await
            }
            Route::ServiceWorker => crate::router::handlers::service_worker_response(&self.state),
            Route::Manifest => crate::router::handlers::manifest_response(&self.state),
            Route::Command => unreachable!("routed to `command`"),
        };
        crate::router::cors::apply(cors, headers, &mut resp);
        resp
    }

    /// `POST /command`: rate limited, body-size limited, JSON only, and never
    /// CORS-exposed, so a third-party page can't invoke app commands.
    async fn command<B>(
        &self,
        method: &Method,
        headers: &HeaderMap,
        peer: Option<std::net::IpAddr>,
        body: B,
    ) -> Resp
    where
        B: http_body::Body<Data = Bytes> + Send,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        if *method != Method::POST {
            return method_not_allowed("POST");
        }
        if let Err(resp) = self.command_limiter.check_request(headers, peer) {
            return *resp;
        }
        if !is_json(headers) {
            return with_type(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "text/plain; charset=utf-8",
                "expected `Content-Type: application/json`",
            );
        }
        let bytes = match Limited::new(body, COMMAND_BODY_LIMIT_BYTES).collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return if e
                    .downcast_ref::<http_body_util::LengthLimitError>()
                    .is_some()
                {
                    bare(StatusCode::PAYLOAD_TOO_LARGE)
                } else {
                    bare(StatusCode::BAD_REQUEST)
                };
            }
        };
        let command = match serde_json::from_slice(&bytes) {
            Ok(command) => command,
            Err(e) => {
                return json(
                    StatusCode::BAD_REQUEST,
                    &crate::router::CommandResponse::err(format!("invalid JSON body: {e}")),
                );
            }
        };
        crate::router::handlers::command_response(&self.state, headers, command)
    }

    /// Everything else: `GET`/`HEAD` for files under the configured static dir.
    async fn static_file(&self, method: &Method, path: &str) -> Resp {
        if *method != Method::GET && *method != Method::HEAD {
            return method_not_allowed("GET, HEAD");
        }
        let Some(mut file) = resolve_static(&self.state.static_dir, path) else {
            return bare(StatusCode::NOT_FOUND);
        };
        if tokio::fs::metadata(&file)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
        {
            file.push("index.html");
        }
        match tokio::fs::read(&file).await {
            Ok(bytes) => with_type(StatusCode::OK, content_type_for(&file), bytes),
            Err(_) => bare(StatusCode::NOT_FOUND),
        }
    }
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(|media| media.trim().eq_ignore_ascii_case("application/json"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Static files
// ---------------------------------------------------------------------------

/// Maps a request path onto `root`, refusing anything that could leave it:
/// `..` segments, backslashes, NULs, and `:` (which gives a drive prefix on
/// Windows). Returns `None` for an unsafe or undecodable path.
pub(crate) fn resolve_static(root: &Path, request_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(request_path)?;
    let mut out = root.to_path_buf();
    for segment in decoded.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.contains(['\\', '\0', ':']) {
            return None;
        }
        out.push(segment);
    }
    Some(out)
}

fn content_type_for(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    }
}

/// Decodes `%XX` escapes. Returns `None` for malformed escapes or non-UTF-8 output.
fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let hex = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Returns the decoded value of query parameter `name`, if present.
pub(crate) fn query_param(query: Option<&str>, name: &str) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (key == name)
            .then(|| percent_decode(&value.replace('+', " ")))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_is_rejected() {
        let root = Path::new("dist");
        assert!(resolve_static(root, "/../secret").is_none());
        assert!(resolve_static(root, "/%2e%2e/secret").is_none());
        assert!(resolve_static(root, "/a\\b").is_none());
        assert!(resolve_static(root, "/C:/windows").is_none());
    }

    #[test]
    fn normal_paths_stay_under_root() {
        let root = Path::new("dist");
        let p = resolve_static(root, "/assets/app.js").unwrap();
        assert!(p.starts_with(root));
        assert!(p.ends_with("app.js"));
    }

    #[test]
    fn percent_decoding_handles_escapes_and_rejects_bad_ones() {
        assert_eq!(percent_decode("a%20b").as_deref(), Some("a b"));
        assert!(percent_decode("bad%zz").is_none());
        assert!(percent_decode("trunc%2").is_none());
    }

    #[test]
    fn query_param_reads_named_value() {
        assert_eq!(
            query_param(Some("x=1&client=ai+agent"), "client").as_deref(),
            Some("ai agent")
        );
        assert_eq!(query_param(Some("x=1"), "client"), None);
        assert_eq!(query_param(None, "client"), None);
    }

    #[test]
    fn content_type_table_covers_wasm_and_js() {
        assert_eq!(content_type_for(Path::new("a.wasm")), "application/wasm");
        assert_eq!(
            content_type_for(Path::new("a.JS")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type_for(Path::new("a.bin")),
            "application/octet-stream"
        );
    }
}
