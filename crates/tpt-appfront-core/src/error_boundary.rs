//! Error boundaries: isolate a subtree so a panic while building it doesn't
//! take down the whole render (Phase 5 / `#61`).
//!
//! A [`Boundary`] wraps a subtree-building closure and a fallback closure. If
//! the primary closure panics while building, the boundary catches the unwind
//! (via [`std::panic::catch_unwind`]) and substitutes the fallback tree
//! instead. This mirrors React's error-boundary idea but at the *tree-build*
//! granularity: AppFront builds its `UITree` synchronously in builder closures,
//! so isolating the build of a subtree is the natural isolation point.
//!
//! Backends are responsible for *invoking* this when they build a subtree that
//! has an attached boundary; the core provides the catch/recover primitive so
//! the logic is backend-agnostic and testable on any target.
//!
//! ```ignore
//! let ui = appfront_core::error_boundary(
//!     || container(|c| c.button("ok").on_click(Msg::Go)),
//!     || container(|c| c.text("something went wrong")),
//! );
//! ```

use crate::UITree;
use std::panic::AssertUnwindSafe;

/// Result of recovering from a boundary: either the primary subtree built fine,
/// or it panicked and we have the fallback (plus the panic message, if one was
/// captured as a `String`/`&str`).
pub enum BoundaryResult<Msg> {
    /// The primary subtree built without panicking.
    Ok(UITree<Msg>),
    /// The primary subtree panicked; this is the fallback subtree.
    Recovered {
        /// The fallback tree produced by the boundary's `fallback` closure.
        fallback: UITree<Msg>,
        /// A best-effort human-readable description of the panic payload, or
        /// `None` if the panic carried a non-string payload.
        panic: Option<String>,
    },
}

/// Builds `primary`, returning it on success. If `primary` panics, runs
/// `fallback` and returns its tree, recording the panic message when it was a
/// string-like payload.
///
/// Note: `catch_unwind` only works if the panic isn't an abort (e.g. a double
/// panic or an `abort`-configured profile). It also requires the closure to be
/// `UnwindSafe`; we wrap in [`AssertUnwindSafe`] because builder closures that
/// only construct a `UITree` are safe to unwind (they don't hold invariants
/// across the call that must be restored on panic).
pub fn error_boundary<Msg: Clone + 'static>(
    primary: impl FnOnce() -> UITree<Msg>,
    fallback: impl FnOnce() -> UITree<Msg>,
) -> BoundaryResult<Msg> {
    match std::panic::catch_unwind(AssertUnwindSafe(primary)) {
        Ok(tree) => BoundaryResult::Ok(tree),
        Err(payload) => {
            let panic = payload_as_string(&payload);
            let fallback = fallback();
            BoundaryResult::Recovered { fallback, panic }
        }
    }
}

/// Like [`error_boundary`] but always returns a `UITree<Msg>`: on panic the
/// fallback tree is returned directly, so callers that don't care about the
/// distinction can use it in place of a normal `|| UITree` builder.
pub fn recover_or<Msg: Clone + 'static>(
    primary: impl FnOnce() -> UITree<Msg>,
    fallback: impl FnOnce() -> UITree<Msg>,
) -> UITree<Msg> {
    match error_boundary(primary, fallback) {
        BoundaryResult::Ok(tree) | BoundaryResult::Recovered { fallback: tree, .. } => tree,
    }
}

fn payload_as_string(payload: &Box<dyn std::any::Any + Send>) -> Option<String> {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContainerBuilder, NodeKind};

    fn button_ok<Msg>() -> UITree<Msg> {
        UITree::container(|c| {
            c.button("ok");
        })
    }

    fn button_boom<Msg>() -> UITree<Msg> {
        UITree::container(|c| {
            c.button("ok");
            panic!("boom");
        })
    }

    fn fallback_tree<Msg>() -> UITree<Msg> {
        UITree::container(|c| {
            c.text("recovered");
        })
    }

    #[test]
    fn primary_ok_returns_primary() {
        let r = error_boundary(button_ok::<()>, fallback_tree::<()>);
        assert!(matches!(r, BoundaryResult::Ok(_)));
    }

    #[test]
    fn panic_recovers_with_fallback() {
        let r = error_boundary(button_boom::<()>, fallback_tree::<()>);
        match r {
            BoundaryResult::Recovered { fallback, panic } => {
                let NodeKind::Container { children } = &fallback.kind else {
                    panic!("expected container");
                };
                assert!(matches!(children[0].kind, NodeKind::Text { .. }));
                assert_eq!(panic.as_deref(), Some("boom"));
            }
            BoundaryResult::Ok(_) => panic!("expected recovery"),
        }
    }

    #[test]
    fn recover_or_returns_tree_either_way() {
        let ok = recover_or(button_ok::<()>, fallback_tree::<()>);
        let recovered = recover_or(button_boom::<()>, fallback_tree::<()>);
        assert!(matches!(ok.kind, NodeKind::Container { .. }));
        assert!(matches!(recovered.kind, NodeKind::Container { .. }));
    }

    #[test]
    fn recovers_with_empty_children_primary() {
        // Ensure unused builder import still compiles in real use.
        let _ = ContainerBuilder::<()>::new();
    }
}
