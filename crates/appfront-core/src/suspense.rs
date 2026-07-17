//! `<Suspense>` boundaries — render a fallback while one or more [`Resource`]s
//! are `Loading`, swapping in the real content once they `Ready`.
//!
//! This is the backend-agnostic half of the async story (see
//! [`crate::resource`] for the `async fn` → `Resource` bridge and
//! cancellation). A `Suspense` boundary is just a `UITree` builder: it inspects
//! the tracked resource(s) and, at build time, emits either the `fallback` or
//! the `content` subtree. Because the resource's signal is reactive, a backend
//! that re-builds the tree on signal change (the DOM/canvas reconcile paths)
//! automatically flips between fallback and content as the fetch settles —
//! no special-casing in the backend required.
//!
//! Cancellation on reload is handled by [`Resource::reload`] / [`Resource::load_async`]
//! (they bump the generation and discard stale results). Cancellation on
//! *unmount* is handled by the backend's existing `unmount` cleanup (the
//! boundary holds only `Signal`-backed `Resource`s, which drop with the tree);
//! a `spawn`-based load that resolves after the subtree is gone sees
//! [`Resource::is_current`] return `false` and silently no-ops, so an unmounted
//! Suspense never commits a stale result into a torn-down view.

use crate::resource::Resource;
use crate::signal::Signal;
use crate::ui_tree::UITree;

/// A `<Suspense>` boundary. Construct with [`Suspense::new`], optionally track
/// one or more [`Resource`]s it should gate on, then `.fallback(...)` /
/// `.content(...)` the two subtrees and call `.build()` to get a `UITree`.
///
/// The boundary is `Ready` (renders `content`) only when **every** tracked
/// resource is in the `Ready` state; otherwise it renders `fallback`. Untracked
/// boundaries are always `Ready`.
pub struct Suspense<Msg> {
    ready_flags: Vec<Signal<bool>>,
    fallback: Box<dyn Fn() -> UITree<Msg>>,
    content: Box<dyn Fn() -> UITree<Msg>>,
}

impl<Msg: Clone + 'static> Suspense<Msg> {
    /// Begins building a boundary that renders `content` once ready.
    pub fn new(content: impl Fn() -> UITree<Msg> + 'static) -> Self {
        Suspense {
            ready_flags: Vec::new(),
            fallback: Box::new(|| UITree::container(|_| {})),
            content: Box::new(content),
        }
    }

    /// Adds a resource the boundary waits on. The boundary only reads the
    /// resource's `Ready` discriminant, captured here as a plain `Signal<bool>`
    /// so a boundary can gate on heterogeneous resource value types without any
    /// casting. The signal is live: when the resource settles (or a backend
    /// re-builds the tree), the gate re-evaluates.
    pub fn track<T: Clone + 'static>(mut self, resource: &Resource<T>) -> Self {
        let ready = resource.is_ready();
        self.ready_flags.push(Signal::new(ready));
        self
    }

    /// Sets the subtree shown while any tracked resource is `Loading`/`Error`
    /// (default: an empty container).
    pub fn fallback(mut self, fallback: impl Fn() -> UITree<Msg> + 'static) -> Self {
        self.fallback = Box::new(fallback);
        self
    }

    /// Whether every tracked resource is `Ready`.
    pub fn is_ready(&self) -> bool {
        self.ready_flags.iter().all(|f| f.get())
    }

    /// Builds the final `UITree`: `content` when ready, else `fallback`.
    pub fn build(&self) -> UITree<Msg> {
        if self.is_ready() {
            (self.content)()
        } else {
            (self.fallback)()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::Resource;
    use crate::ui_tree::{ContainerBuilder, NodeKind};

    #[test]
    fn untracked_boundary_is_always_ready() {
        let s = Suspense::new(|| UITree::container(|_: &mut ContainerBuilder<()>| {}));
        assert!(s.is_ready());
        let tree = s.fallback(|| panic!("should not be used")).build();
        assert!(matches!(tree.kind, NodeKind::Container { .. }));
    }

    #[test]
    fn shows_fallback_while_loading_then_content_when_ready() {
        let data = Resource::<String>::new(|| Err("loading".to_string()));
        let s = Suspense::new(|| UITree::container(|c: &mut ContainerBuilder<()>| { c.text("loaded"); }))
            .track(&data)
            .fallback(|| UITree::container(|c: &mut ContainerBuilder<()>| { c.text("loading..."); }))
            .build();
        match s.kind {
            NodeKind::Container { children } => match &children[0].kind {
                NodeKind::Text { text } => assert_eq!(text, "loading..."),
                _ => panic!("expected fallback text"),
            },
            _ => panic!("expected container"),
        }

        data.set_result(Ok("done".to_string()));
        let s2 = Suspense::new(|| UITree::container(|c: &mut ContainerBuilder<()>| { c.text("loaded"); }))
            .track(&data)
            .fallback(|| UITree::container(|c: &mut ContainerBuilder<()>| { c.text("loading..."); }))
            .build();
        match s2.kind {
            NodeKind::Container { children } => match &children[0].kind {
                NodeKind::Text { text } => assert_eq!(text, "loaded"),
                _ => panic!("expected content text"),
            },
            _ => panic!("expected container"),
        }
    }

    #[test]
    fn gates_on_all_tracked_resources() {
        let a = Resource::ready(1i32);
        let b = Resource::<i32>::new(|| Err("loading".to_string()));
        let s = Suspense::new(|| UITree::container(|_: &mut ContainerBuilder<()>| {}))
            .track(&a)
            .track(&b);
        assert!(!s.is_ready(), "boundary waits on the still-loading resource");
    }
}
