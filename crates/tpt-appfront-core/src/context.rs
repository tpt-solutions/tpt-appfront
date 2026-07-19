//! Tree-scoped shared state (a lightweight Context/DI primitive).
//!
//! Deeply-nested components often need shared state (theme, current user, a
//! router) without threading a `Signal` through every constructor argument.
//! This module provides a `Context<T>` plus a provider stack so a component
//! can *provide* a value and any descendant can *consume* the nearest one:
//!
//! ```ignore
//! let theme = Context::new(Signal::new(Theme::Dark));
//! provide_context(&theme, || {
//!     // inside here, `use_context::<Theme>()` returns `theme`
//!     let t = use_context::<Theme>();
//!     container(|c| c.text(format!("theme: {:?}", t.get())))
//! });
//! ```
//!
//! The stack is thread-local and keyed by the value's type `T`, so a provider
//! shadows any outer provider of the same type for the duration of its scope
//! closure. This is backend-agnostic: it neither reads nor writes the DOM,
//! canvas, or any `UITree` field — it only coordinates during the synchronous
//! build of the tree (builder closures run nested on the call stack, which is
//! exactly the scope a provider should cover).

use crate::signal::Signal;
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A piece of shared, reactive state that can be provided to a subtree and
/// consumed by any descendant.
///
/// Internally wraps a [`Signal<T>`] so consumers see updates live, exactly like
/// any other signal in the reactive core.
pub struct Context<T> {
    signal: Signal<T>,
}

impl<T> Clone for Context<T> {
    fn clone(&self) -> Self {
        Context {
            signal: self.signal.clone(),
        }
    }
}

impl<T: Clone + 'static> Context<T> {
    /// Creates a context from an initial value.
    pub fn new(value: T) -> Self {
        Context {
            signal: Signal::new(value),
        }
    }

    /// Creates a context directly around an existing [`Signal`].
    pub fn from_signal(signal: Signal<T>) -> Self {
        Context { signal }
    }

    /// Reads the current value.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.signal.get()
    }

    /// Updates the value.
    pub fn set(&self, value: T) {
        self.signal.set(value);
    }

    /// Returns the underlying signal so consumers can subscribe to changes.
    pub fn signal(&self) -> Signal<T> {
        self.signal.clone()
    }
}

thread_local! {
    /// Per-type stack of providers. The top of each type's stack is the
    /// nearest provider visible to code currently running.
    static PROVIDERS: RefCell<HashMap<TypeId, Vec<Rc<dyn Any>>>> =
        RefCell::new(HashMap::new());
}

/// Provides `ctx` to the subtree built inside `scope`, then restores the
/// previous provider afterwards. Any `use_context::<T>()` call made while
/// `scope` runs resolves to `ctx`.
pub fn provide_context<T: 'static>(ctx: &Context<T>, scope: impl FnOnce()) {
    let key = TypeId::of::<T>();
    PROVIDERS.with(|p| {
        p.borrow_mut()
            .entry(key)
            .or_insert_with(Vec::new)
            .push(Rc::new(ctx.clone()) as Rc<dyn Any>);
    });
    scope();
    PROVIDERS.with(|p| {
        let mut map = p.borrow_mut();
        if let Some(stack) = map.get_mut(&key) {
            stack.pop();
            if stack.is_empty() {
                map.remove(&key);
            }
        }
    });
}

/// Returns the nearest [`Context<T>`] provided by an enclosing
/// [`provide_context`], or `None` if no provider of type `T` is in scope.
pub fn use_context<T: 'static>() -> Option<Context<T>> {
    let key = TypeId::of::<T>();
    PROVIDERS.with(|p| {
        let stack = p.borrow();
        stack
            .get(&key)
            .and_then(|s| s.last())
            .and_then(|rc| rc.downcast_ref::<Context<T>>())
            .cloned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Theme {
        dark: bool,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct User {
        name: String,
    }

    #[test]
    fn use_context_returns_none_when_unprovided() {
        assert!(use_context::<Theme>().is_none());
    }

    #[test]
    fn provide_then_use_resolves_nearest() {
        let outer = Context::new(Theme { dark: false });
        let mut observed = None;
        provide_context(&outer, || {
            observed = use_context::<Theme>().map(|c| c.get());
        });
        assert_eq!(observed, Some(Theme { dark: false }));
    }

    #[test]
    fn inner_provider_shadows_outer() {
        let outer = Context::new(Theme { dark: false });
        let inner = Context::new(Theme { dark: true });
        let mut outer_before = None;
        let mut inner_seen = None;
        let mut outer_after = None;
        provide_context(&outer, || {
            outer_before = use_context::<Theme>().map(|c| c.get());
            provide_context(&inner, || {
                inner_seen = use_context::<Theme>().map(|c| c.get());
            });
            // After the inner scope ends, the outer provider is visible again.
            outer_after = use_context::<Theme>().map(|c| c.get());
        });
        assert_eq!(inner_seen, Some(Theme { dark: true }));
        assert_eq!(outer_before, Some(Theme { dark: false }));
        assert_eq!(outer_after, Some(Theme { dark: false }));
    }

    #[test]
    fn different_types_are_independent() {
        let theme = Context::new(Theme { dark: true });
        let user = Context::new(User {
            name: "ada".to_string(),
        });
        provide_context(&theme, || {
            provide_context(&user, || {
                assert!(use_context::<Theme>().is_some());
                assert!(use_context::<User>().is_some());
                assert!(use_context::<i32>().is_none());
            });
        });
    }

    #[test]
    fn context_updates_are_visible_to_consumers() {
        let theme = Context::new(Theme { dark: false });
        let observed = Rc::new(RefCell::new(None));
        let obs = observed.clone();
        provide_context(&theme, || {
            if let Some(c) = use_context::<Theme>() {
                *obs.borrow_mut() = Some(c.get());
                c.set(Theme { dark: true });
                *obs.borrow_mut() = Some(c.get());
            }
        });
        assert_eq!(*observed.borrow(), Some(Theme { dark: true }));
    }
}
