//! Component model primitives shared across backends.
//!
//! `appfront`'s `#[component]` macro (re-exported as
//! `tpt_appfront_core::component`) builds on these types to give components a
//! real, React-like shape: typed `Props`, `children` slots, and optional
//! memoization keyed on props equality.

use crate::ui_tree::UITree;
use std::cell::RefCell;
use std::collections::HashMap;

/// The `children` slot passed to a component that accepts them. A component
/// declares a `children: Children<Msg>` parameter and renders the passed
/// subtrees somewhere in its own tree:
///
/// ```ignore
/// #[tpt_appfront_core::component]
/// fn card(props: CardProps, children: Children<Msg>) -> UITree<Msg> {
///     UITree::container(|c| {
///         c.heading(2, &props.title);
///         for child in children.0 {
///             c.with(child);
///         }
///     })
/// }
/// ```
///
/// `Children` is just a typed `Vec<UITree<Msg>>` so a component can iterate and
/// re-emit the slot content with full control over where it lands.
#[derive(Debug, Clone, Default)]
pub struct Children<Msg>(pub Vec<UITree<Msg>>);

impl<Msg> Children<Msg> {
    /// An empty slot.
    pub fn none() -> Self {
        Children(Vec::new())
    }

    /// Whether the slot carries no children.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of children in the slot.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

thread_local! {
    // The cache stores the last `(key, tree)` per component id. Both `P` (the
    // memo key) and `Msg` are type-erased via `Box<dyn Any>`; `memoize` only
    // ever downcasts to its own concrete `(P, UITree<Msg>)`, which is sound
    // because each component id is generated once with a fixed `(P, Msg)`.
    static MEMO_CACHE: RefCell<HashMap<u64, Box<dyn std::any::Any>>> =
        RefCell::new(HashMap::new());
}

/// Memoize `build(key)` for the component identified by `id`. If the previous
/// `key` for `id` is `PartialEq` to the current one, the cached `UITree<Msg>`
/// is cloned and returned without calling `build`, preserving the previously
/// built `Msg`-bound event closures. Otherwise `build` runs, its result is
/// cached, and it is returned.
///
/// `P` must be `PartialEq + Clone + 'static`. This is the primitive behind
/// `#[component(memo)]`.
pub fn memoize<P, Msg, F>(id: u64, key: P, build: F) -> UITree<Msg>
where
    P: PartialEq + Clone + 'static,
    Msg: Clone + 'static,
    F: FnOnce(&P) -> UITree<Msg>,
{
    MEMO_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(entry) = cache.get(&id) {
            if let Some((prev_key, prev_tree)) =
                entry.downcast_ref::<(P, UITree<Msg>)>()
            {
                if *prev_key == key {
                    return prev_tree.clone();
                }
            }
        }
        let tree = build(&key);
        cache.insert(id, Box::new((key, tree.clone())) as Box<dyn std::any::Any>);
        tree
    })
}
