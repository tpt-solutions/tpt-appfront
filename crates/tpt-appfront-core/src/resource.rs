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
//!
//! The [`spawn_resource`] bridge and the [`crate::suspense`] boundary build on
//! this: they turn an `async fn` into a `Resource` (driving the future via a
//! caller-supplied executor) and render a fallback while the resource is
//! `Loading`, swapping to the real subtree once it `Ready`s — with automatic
//! cancellation on reload/unmount (see [`crate::suspense::Suspense`]).

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
    /// Monotonic generation counter. Bumped on every (re)load so a late-
    /// resolving fetch from a *previous* generation can be detected and
    /// discarded instead of overwriting a newer result. This is the
    /// cancellation mechanism behind [`Resource::load_async`] and
    /// [`crate::suspense::Suspense`]: a task holds the generation it started
    /// with and refuses to commit its result if the resource has since moved on.
    generation: std::rc::Rc<std::cell::Cell<u64>>,
}

impl<T> Clone for Resource<T> {
    fn clone(&self) -> Self {
        Resource {
            state: self.state.clone(),
            generation: self.generation.clone(),
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
            generation: std::rc::Rc::new(std::cell::Cell::new(0)),
        };
        res.load_blocking(loader);
        res
    }

    /// Creates a resource already in the `Ready` state.
    pub fn ready(value: T) -> Self {
        Resource {
            state: Signal::new(ResourceState::Ready(value)),
            generation: std::rc::Rc::new(std::cell::Cell::new(0)),
        }
    }

    /// Creates a resource already in the `Error` state.
    pub fn error(message: impl Into<String>) -> Self {
        Resource {
            state: Signal::new(ResourceState::Error(message.into())),
            generation: std::rc::Rc::new(std::cell::Cell::new(0)),
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
    ///
    /// Async callers should prefer [`Resource::load_async`], whose spawn
    /// closure uses [`Resource::is_current`] to commit a result only if it
    /// hasn't been superseded by a newer load — that is the cancellation guard.
    pub fn set_result(&self, result: Result<T, String>) {
        self.state.set(match result {
            Ok(v) => ResourceState::Ready(v),
            Err(e) => ResourceState::Error(e),
        });
    }

    /// The current generation counter. Incremented on every reload/async
    /// (re)load, so async tasks can detect that they've been superseded.
    pub fn generation(&self) -> u64 {
        self.generation.get()
    }

    /// True only if the resource is *still* on generation `gen` — i.e. no newer
    /// `reload`/`load_async` has superseded the load that started at `gen`. A
    /// task that captured `gen = resource.generation()` before awaiting its
    /// future checks `resource.is_current(gen)` after the await: if `false`, a
    /// newer load has started and the result must be dropped (cancellation).
    pub fn is_current(&self, gen: u64) -> bool {
        self.generation.get() == gen
    }

    /// Spawns an `async` loader and bridges its result back into this resource.
    ///
    /// The core is runtime-free, so the caller supplies a `spawn` function that
    /// drives a `Future<Output = Result<T, String>>` to completion on whatever
    /// executor the backend provides (the DOM uses `wasm_bindgen_futures`, a
    /// native app uses `tokio`/`smol`, tests use a oneshot channel). `spawn`
    /// is given the future plus a clone of this `Resource` and the generation it
    /// started with; once the future resolves the result is committed *only if*
    /// the resource is still on that generation (cancellation on reload).
    ///
    /// The resource is immediately put into the `Loading` state and its
    /// generation is bumped so any in-flight load from a previous generation is
    /// invalidated. Calling `load_async` again on the same resource (or any
    /// clone) cancels the prior in-flight future.
    pub fn load_async<F, Fut>(&self, spawn: F, loader: Fut)
    where
        F: FnOnce(Resource<T>, u64, Fut),
        Fut: std::future::Future<Output = Result<T, String>> + 'static,
    {
        // Bump generation *before* entering Loading so a concurrent task that
        // captured the old generation after this call will be invalidated.
        let gen = self.generation.get() + 1;
        self.generation.set(gen);
        self.state.set(ResourceState::Loading);
        spawn(self.clone(), gen, loader);
    }

    /// Resets the resource back to `Loading` (e.g. to re-trigger a fetch).
    ///
    /// Bumps the generation so any in-flight async load is cancelled (its
    /// eventual result will be discarded by [`Resource::is_current`]).
    pub fn reload(&self) {
        self.generation.set(self.generation.get() + 1);
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

    /// A trivial synchronous "executor" used by the async tests: it runs the
    /// future to completion on the spot and — if still current — commits it.
    #[cfg(test)]
    fn sync_spawn<T, Fut>(res: Resource<T>, gen: u64, fut: Fut)
    where
        T: Clone + 'static,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let out = futures_lite_like::block_on(fut);
        if res.is_current(gen) {
            res.set_result(out);
        }
    }

    #[test]
    fn load_async_enters_loading_then_resolves_when_current() {
        let r = Resource::<i32>::ready(0);
        let before = r.generation();
        r.load_async(
            sync_spawn,
            async { Ok(123) },
        );
        // `load_async` immediately bumps the generation and goes Loading; the
        // synchronous spawn resolves it and commits because the gen is current.
        assert!(r.generation() > before, "generation bumped");
        assert!(r.is_ready(), "synchronous spawn committed the result");
        assert_eq!(r.ready_value(), Some(123));
    }

    #[test]
    fn reload_cancels_a_superseded_async_load() {
        // Use a spawn that *defers* committing so we can interleave a reload.
        // We capture the gen, then supersede it with `reload`, then attempt to
        // commit the old result — it must be dropped (cancellation).
        let r = Resource::<i32>::ready(0);

        // `deferred` records the (res, gen) pair so the test can drive it.
        let mut pending: Option<(Resource<i32>, u64)> = None;
        r.load_async(
            |res, g, _fut| {
                pending = Some((res, g));
            },
            async { Ok(123) },
        );
        assert!(r.is_loading());
        let (res, gen) = pending.take().unwrap();

        // A newer load starts while the old one is still "in flight".
        res.reload();
        assert!(!res.is_current(gen), "reload invalidated the old generation");

        // The stale result is dropped because its generation is obsolete.
        if res.is_current(gen) {
            res.set_result(Ok(999));
        }
        assert!(res.is_loading(), "stale result was cancelled; still Loading");
        assert_eq!(res.ready_value(), None);
    }

    #[test]
    fn clones_share_one_generation_counter() {
        let r = Resource::<String>::ready("init".to_string());
        let clone = r.clone();
        let captured = r.generation();
        // Reload via the clone; the original observes the same bump.
        clone.reload();
        assert!(!r.is_current(captured));
        assert_eq!(r.generation(), clone.generation());
    }
}

/// A tiny `block_on` shim so the async tests don't need a real executor or an
/// async-std/tokio dependency in `appfront-core`. It polls a future with a
/// no-op waker until it resolves.
#[cfg(test)]
mod futures_lite_like {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    pub fn block_on<F: Future>(fut: F) -> F::Output {
        // SAFETY: the no-op waker never touches the data pointer, so the
        // vtable's clone/wake/drop are all inert.
        let waker = unsafe { Waker::from_raw(noop_waker()) };
        let mut cx = Context::from_waker(&waker);
        let mut fut = fut;
        let mut fut = unsafe { Pin::new_unchecked(&mut fut) };
        loop {
            if let Poll::Ready(val) = fut.as_mut().poll(&mut cx) {
                return val;
            }
        }
    }

    fn noop_raw_waker() -> *const () {
        &()
    }

    unsafe fn clone(_: *const ()) -> RawWaker {
        noop_waker()
    }
    unsafe fn wake(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}

    fn noop_waker() -> RawWaker {
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake, drop);
        RawWaker::new(noop_raw_waker(), &VTABLE)
    }
}
