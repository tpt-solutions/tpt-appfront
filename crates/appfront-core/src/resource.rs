//! Async data primitive (`Resource<T>`) — a SolidJS/Svelte-store-style wrapper
//! for async fetches that exposes `Loading` / `Ready` / `Error` states through
//! the reactive [`Signal`] core, so UI code doesn't hand-roll ad-hoc loading
//! flags.
//!
//! The loader is synchronous-from-the-core's perspective: callers feed either a
//! blocking loader (`Resource::new`) or an already-resolved `Result`
//! (`Resource::ready` / `Resource::error`). Frontends that drive a real async
//! runtime (the DOM/canvas backends, or an app's own executor) call
//! [`Resource::load_blocking`] inside their async task and then [`Resource::set_result`]
//! once the future resolves — the resource's signal updates and any subscribed
//! view re-renders. Keeping the core runtime-free is intentional: the same
//! `Resource` works identically on every backend.

use crate::signal::Signal;
use std::fmt::Debug;

/// The lifecycle state of a [`Resource`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceState<T> {
    /// Loader has not completed yet.
    Loading,
    /// Loader succeeded with a value.
    Ready(T),
    /// Loader failed with a message.
    Error(String),
}

/// A reactive handle to an async-loaded value.
///
/// Clone is cheap (internally `Rc`-backed via [`Signal`]); all clones share the
/// same underlying state, so providing a `Resource` to a subtree and updating
/// it from a task updates every consumer.
pub struct Resource<T> {
    state: Signal<ResourceState<T>>,
}

impl<T> Clone for Resource<T> {
    fn clone(&self) -> Self {
        Resource {
            state: self.state.clone(),
        }
    }
}

impl<T: Clone + 'static> Resource<T> {
    /// Creates a resource in the `Loading` state, then runs `loader`
    /// synchronously and stores the result. Useful for "load on creation"
    /// cases where the loader is already resolved (e.g. a cached value or a
    /// blocking fetch driven by the caller's executor).
    pub fn new(loader: impl FnOnce() -> Result<T, String>) -> Self {
        let res = Resource {
            state: Signal::new(ResourceState::Loading),
        };
        res.load_blocking(loader);
        res
    }

    /// Creates a resource already in the `Ready` state.
    pub fn ready(value: T) -> Self {
        Resource {
            state: Signal::new(ResourceState::Ready(value)),
        }
    }

    /// Creates a resource already in the `Error` state.
    pub fn error(message: impl Into<String>) -> Self {
        Resource {
            state: Signal::new(ResourceState::Error(message.into())),
        }
    }

    /// Runs `loader` and updates the shared state to `Ready`/`Error`. Safe to
    /// call from any thread/task that can reach this `Resource` (e.g. the
    /// continuation of an awaited future).
    pub fn load_blocking(&self, loader: impl FnOnce() -> Result<T, String>) {
        let next = match loader() {
            Ok(v) => ResourceState::Ready(v),
            Err(e) => ResourceState::Error(e),
        };
        self.state.set(next);
    }

    /// Updates the state directly (e.g. from an already-resolved future).
    pub fn set_result(&self, result: Result<T, String>) {
        self.state.set(match result {
            Ok(v) => ResourceState::Ready(v),
            Err(e) => ResourceState::Error(e),
        });
    }

    /// Resets the resource back to `Loading` (e.g. to re-trigger a fetch).
    pub fn reload(&self) {
        self.state.set(ResourceState::Loading);
    }

    /// The current state.
    pub fn state(&self) -> ResourceState<T> {
        self.state.get()
    }

    /// Convenience accessor: the ready value, or `None` while loading/errored.
    pub fn ready_value(&self) -> Option<T> {
        match self.state.get() {
            ResourceState::Ready(v) => Some(v),
            _ => None,
        }
    }

    /// `true` while the loader has not completed.
    pub fn is_loading(&self) -> bool {
        matches!(self.state.get(), ResourceState::Loading)
    }

    /// `true` once the loader has succeeded.
    pub fn is_ready(&self) -> bool {
        matches!(self.state.get(), ResourceState::Ready(_))
    }

    /// `true` if the loader failed.
    pub fn is_error(&self) -> bool {
        matches!(self.state.get(), ResourceState::Error(_))
    }

    /// The underlying state signal, so views can subscribe to changes.
    pub fn signal(&self) -> Signal<ResourceState<T>> {
        self.state.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_runs_loader_and_stores_value() {
        let r = Resource::new(|| Ok(42));
        assert!(r.is_ready());
        assert_eq!(r.ready_value(), Some(42));
        assert!(!r.is_loading());
        assert!(!r.is_error());
    }

    #[test]
    fn new_captures_loader_error() {
        let r: Resource<i32> = Resource::new(|| Err("boom".to_string()));
        assert!(r.is_error());
        assert_eq!(r.state(), ResourceState::Error("boom".to_string()));
    }

    #[test]
    fn ready_and_error_constructors() {
        assert_eq!(Resource::<i32>::ready(7).ready_value(), Some(7));
        assert!(Resource::<i32>::error("nope").is_error());
    }

    #[test]
    fn load_blocking_updates_state_and_clones_share_it() {
        let r = Resource::ready(1);
        let clone = r.clone();
        r.load_blocking(|| Ok(99));
        assert_eq!(clone.ready_value(), Some(99));
    }

    #[test]
    fn reload_resets_to_loading() {
        let r = Resource::ready(5);
        r.reload();
        assert!(r.is_loading());
    }

    #[test]
    fn set_result_stores_error() {
        let r = Resource::new(|| Ok(1));
        r.set_result(Err("late failure".to_string()));
        assert!(r.is_error());
    }
}
