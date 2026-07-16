//! Backend-agnostic client-side router.
//!
//! A real hash/history-based router is more than the bare [`route_signal`]
//! pointer in [`crate::agent`] (which exists for AI-agent/devtools purposes).
//! This module adds a route *table* with path-matching and parameter
//! extraction, plus a [`Router`] that owns the current location, resolves it
//! to a view, and notifies subscribers on navigation.
//!
//! The router is intentionally generic over the app `Msg` type and holds no
//! browser-only APIs — backends wire real navigation (History API, hashchange)
//! to [`Router::navigate`]. `appfront-dom` does this over the History API on
//! `wasm32`; `appfront-html` / `appfront-ai-schema` resolve routes at
//! crawl/generation time.

use crate::signal::Signal;
use crate::ui_tree::UITree;
use std::collections::HashMap;
use std::rc::Rc;

/// A view-producing handler for a matched route. Receives the captured path
/// params and returns the [`UITree`] for that route.
pub type RouteHandler<Msg> = Rc<dyn Fn(&HashMap<String, String>) -> UITree<Msg>>;

/// A compiled route pattern.
///
/// Patterns use `:name` segments to capture a single path component into the
/// params map (e.g. `/users/:id` matches `/users/42` with `id = "42"`). A
/// trailing `*` wildcard is not supported in v1.
#[derive(Debug, Clone)]
pub struct Route {
    raw: String,
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// A literal path component that must match exactly.
    Literal(String),
    /// A `:name` capture; the matched component is stored under `name`.
    Param(String),
}

impl Route {
    /// Parses a route pattern. Returns `Err` if a segment is empty or a
    /// `:param` has no name.
    pub fn parse(pattern: &str) -> Result<Route, String> {
        let trimmed = pattern.trim_matches('/');
        let segments = if trimmed.is_empty() {
            Vec::new()
        } else {
            trimmed
                .split('/')
                .map(|seg| {
                    if seg.is_empty() {
                        return Err(format!("empty segment in route `{pattern}`"));
                    }
                    if let Some(name) = seg.strip_prefix(':') {
                        if name.is_empty() {
                            return Err(format!("unnamed `:` param in route `{pattern}`"));
                        }
                        Ok(Segment::Param(name.to_string()))
                    } else {
                        Ok(Segment::Literal(seg.to_string()))
                    }
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        Ok(Route {
            raw: pattern.to_string(),
            segments,
        })
    }

    /// The original pattern string this route was parsed from.
    pub fn pattern(&self) -> &str {
        &self.raw
    }

    /// Attempts to match `path` (a `/`-prefixed path), returning the captured
    /// params on success.
    pub fn match_path(&self, path: &str) -> Option<HashMap<String, String>> {
        let trimmed = path.trim_matches('/');
        let incoming: Vec<&str> = if trimmed.is_empty() {
            Vec::new()
        } else {
            trimmed.split('/').collect()
        };

        // The root route (`/`) matches only the empty path.
        if self.segments.is_empty() {
            return if incoming.is_empty() {
                Some(HashMap::new())
            } else {
                None
            };
        }

        if incoming.len() != self.segments.len() {
            return None;
        }
        let mut params = HashMap::new();
        for (seg, raw) in self.segments.iter().zip(incoming.iter()) {
            match seg {
                Segment::Literal(lit) => {
                    if lit != raw {
                        return None;
                    }
                }
                Segment::Param(name) => {
                    params.insert(name.clone(), (*raw).to_string());
                }
            }
        }
        Some(params)
    }
}

/// A route table mapping patterns to view-producing handlers.
///
/// Handlers receive the matched path params and return the [`UITree`] for that
/// route. The first registered route wins on ambiguity, so register more
/// specific routes before catch-alls.
pub struct RouteTable<Msg> {
    routes: Vec<(Route, RouteHandler<Msg>)>,
    /// View returned when no route matches.
    not_found: Option<Rc<dyn Fn() -> UITree<Msg>>>,
}

impl<Msg> Default for RouteTable<Msg> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Msg> RouteTable<Msg> {
    /// Creates an empty table.
    pub fn new() -> Self {
        RouteTable {
            routes: Vec::new(),
            not_found: None,
        }
    }

    /// Registers a route. `pattern` uses `:param` captures; `handler` produces
    /// the view for a matched route given its params.
    pub fn route<F>(mut self, pattern: &str, handler: F) -> Result<Self, String>
    where
        F: Fn(&HashMap<String, String>) -> UITree<Msg> + 'static,
    {
        let route = Route::parse(pattern)?;
        self.routes.push((route, Rc::new(handler)));
        Ok(self)
    }

    /// Sets the not-found view used when no route matches.
    pub fn fallback<F>(mut self, handler: F) -> Self
    where
        F: Fn() -> UITree<Msg> + 'static,
    {
        self.not_found = Some(Rc::new(handler));
        self
    }

    /// Resolves `path` to a view, or the not-found view if nothing matches.
    pub fn resolve(&self, path: &str) -> UITree<Msg> {
        for (route, handler) in &self.routes {
            if let Some(params) = route.match_path(path) {
                return handler(&params);
            }
        }
        if let Some(fb) = &self.not_found {
            return fb();
        }
        // Last-resort empty container so callers always get a renderable tree.
        UITree::container(|_| {})
    }
}

/// A reactive router that owns the current location and re-resolves the route
/// table on navigation.
///
/// `Router` is cheap to clone (internally `Rc`-backed) and integrates with the
/// existing reactive [`Signal`] so effects subscribed to [`Router::signal`]
/// re-run on every navigation — that is how a backend rebuilds the UI tree.
pub struct Router<Msg> {
    inner: Rc<RouterInner<Msg>>,
}

struct RouterInner<Msg> {
    table: RouteTable<Msg>,
    location: Signal<String>,
}

impl<Msg> Clone for Router<Msg> {
    fn clone(&self) -> Self {
        Router {
            inner: self.inner.clone(),
        }
    }
}

impl<Msg> Router<Msg> {
    /// Builds a router from a [`RouteTable`], starting at `initial_path`.
    pub fn new(table: RouteTable<Msg>, initial_path: &str) -> Self {
        Router {
            inner: Rc::new(RouterInner {
                table,
                location: Signal::new(initial_path.to_string()),
            }),
        }
    }

    /// The current path.
    pub fn current_path(&self) -> String {
        self.inner.location.get()
    }

    /// Navigates to `path`, updating the location signal (which re-triggers any
    /// subscribed effects). A backend typically also syncs the browser URL.
    pub fn navigate(&self, path: &str) {
        self.inner.location.set(path.to_string());
    }

    /// Returns the location signal so effects can subscribe to navigation.
    pub fn signal(&self) -> Signal<String> {
        self.inner.location.clone()
    }

    /// Resolves the current path to a view.
    pub fn current_view(&self) -> UITree<Msg> {
        self.inner.table.resolve(&self.inner.location.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_tree::{ContainerBuilder, NodeKind};

    #[test]
    fn route_match_literal_and_root() {
        let r = Route::parse("/").unwrap();
        assert!(r.match_path("/").is_some());
        assert!(r.match_path("/x").is_none());

        let r = Route::parse("/about").unwrap();
        assert!(r.match_path("/about").is_some());
        assert!(r.match_path("/").is_none());
        assert!(r.match_path("/about/extra").is_none());
    }

    #[test]
    fn route_match_param_capture() {
        let r = Route::parse("/users/:id").unwrap();
        let m = r.match_path("/users/42").unwrap();
        assert_eq!(m.get("id"), Some(&"42".to_string()));
        assert!(r.match_path("/users").is_none());
        assert!(r.match_path("/posts/42").is_none());
    }

    #[test]
    fn route_table_resolves_and_falls_back() {
        let table = RouteTable::<()>::new()
            .route("/", |_| UITree::container(|_| {}))
            .unwrap()
            .route("/users/:id", |p| {
                let _ = p;
                UITree::container(|b: &mut ContainerBuilder<()>| {
                    b.text(format!("user {}", p.get("id").unwrap()));
                })
            })
            .unwrap()
            .fallback(|| UITree::container(|_| {}));

        assert!(matches!(table.resolve("/"), UITree { kind: NodeKind::Container { .. }, .. }));
        let view = table.resolve("/users/7");
        assert!(matches!(view, UITree { kind: NodeKind::Container { .. }, .. }));
        // Unknown route hits the fallback.
        assert!(matches!(table.resolve("/nope"), UITree { kind: NodeKind::Container { .. }, .. }));
    }

    #[test]
    fn router_navigation_updates_view_via_signal() {
        let table = RouteTable::<()>::new()
            .route("/", |_| UITree::container(|_| {}))
            .unwrap()
            .route("/b", |_| UITree::container(|_| {}))
            .unwrap();
        let router = Router::new(table, "/");
        assert_eq!(router.current_path(), "/");

        let sig = router.signal();
        let before = sig.get();
        router.navigate("/b");
        assert_eq!(sig.get(), "/b");
        assert_ne!(before, sig.get());
    }

    #[test]
    fn route_parse_rejects_bad_patterns() {
        // A bare `:` with no name is rejected.
        assert!(Route::parse("/:").is_err());
        // Trailing slashes are normalised away, so `/users/` == `/users`.
        assert!(Route::parse("/users/").is_ok());
    }
}
