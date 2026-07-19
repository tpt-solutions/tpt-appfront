//! Compile-time memoization of *static* `UITree` subtrees.
//!
//! A "static" subtree is one whose structure and every text/attribute value is
//! known at compile time (no `{ expr }` interpolation in the `view!` macro).
//! Because it never changes between renders, there is no reason to rebuild it
//! every frame — so [`static_node`] builds it exactly once and returns a `clone`
//! of the cached instance on every later call.
//!
//! This is the concrete payoff of the `#[appfront::component]` /
//! `appfront::view!` `is_dynamic` analysis: instead of only *flagging*
//! dynamic-ness as a hint, the codegen emits `static_node(...)` calls for the
//! provably-static parts of the tree, so backends that rebuild the `UITree`
//! every frame (canvas' immediate-mode `build_ui`, DOM hydration) skip that
//! work for inert content.

use crate::UITree;
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

thread_local! {
    /// `id -> cached tree` for the process. Keyed by a per-node unique id the
    /// macro synthesizes (the address of a generated `static` sentinel), so two
    /// distinct `view!` invocations can never collide.
    static CACHE: RefCell<HashMap<u64, Rc<dyn Any>>> = RefCell::new(HashMap::new());
}

/// Builds `build()` exactly once and caches the result; subsequent calls return
/// a `clone()` of the cached `UITree<Msg>`. `id` must be a stable, globally
/// unique identifier for this static subtree — the macro generates one per
/// static node (the address of a synthesized `static` sentinel), never a bare
/// counter, because a bare counter would collide across separate `view!` calls
/// sharing this one cache.
///
/// Panics on the (impossible) case that `id` was reused with a different `Msg`,
/// because the cache stores `Rc<dyn Any>` keyed by `id` and a type mismatch
/// would mean the macro assigned the same id twice.
pub fn static_node<Msg: Clone + 'static>(
    id: u64,
    build: impl FnOnce() -> UITree<Msg>,
) -> UITree<Msg> {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(existing) = cache.get(&id) {
            let rc = existing.clone();
            if let Ok(tree) = rc.downcast::<UITree<Msg>>() {
                return (*tree).clone();
            }
            // Same id, different type — the macro generated a duplicate id.
            panic!("appfront static_tree: duplicate static node id {id}");
        }
        let tree = build();
        cache.insert(id, Rc::new(tree.clone()));
        tree
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeKind, UITree};

    #[test]
    fn build_runs_exactly_once_per_id() {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let calls2 = std::rc::Rc::clone(&calls);
        let built = static_node(1, || {
            calls2.set(calls2.get() + 1);
            UITree::<()>::container(|c| {
                c.text("hello");
            })
        });
        // First call must have built it.
        assert_eq!(calls.get(), 1);
        assert!(matches!(built.kind, NodeKind::Container { .. }));

        // Subsequent calls return clones without rebuilding.
        for _ in 0..5 {
            let _again = static_node(1, || {
                panic!("build must not run again for a cached id");
                #[allow(unreachable_code)]
                UITree::<()>::container(|c| {
                    c.text("never");
                })
            });
        }
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn distinct_ids_cache_independently() {
        let a = static_node(100, || UITree::<()>::container(|c| { c.text("a"); }));
        let b = static_node(200, || UITree::<()>::container(|c| { c.text("b"); }));
        match (&a.kind, &b.kind) {
            (NodeKind::Container { children: ca }, NodeKind::Container { children: cb }) => {
                match (&ca[0].kind, &cb[0].kind) {
                    (NodeKind::Text { text: ta }, NodeKind::Text { text: tb }) => {
                        assert_eq!(ta, "a");
                        assert_eq!(tb, "b");
                    }
                    _ => panic!("expected text nodes"),
                }
            }
            _ => panic!("expected container nodes"),
        }
    }

    #[test]
    fn cached_tree_is_independent_clone() {
        let first = static_node(300, || UITree::<()>::container(|c| { c.text("x"); }));
        let second = static_node(300, || {
            panic!("build must not run again for a cached id");
            #[allow(unreachable_code)]
            UITree::<()>::container(|c| { c.text("should-not-build"); })
        });
        // Both reference the same cached instance's data after cloning, so the
        // content matches the first build, not the (never-run) second closure.
        assert_eq!(format!("{first:?}"), format!("{second:?}"));
    }
}

