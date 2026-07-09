//! A minimal SolidJS-style reactive signal system.
//!
//! `Signal::get` records itself as a dependency of whichever `Effect` is
//! currently running (tracked via a thread-local stack), so `Signal::set`
//! only re-runs the effects that actually read that signal — no diffing.
//! Dependencies are recomputed on every run (not just accumulated), so an
//! effect that branches (`if cond.get() { a.get() } else { b.get() }`)
//! only stays subscribed to whichever branch it last took.

use serde::de::DeserializeOwned;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

type EffectFn = dyn FnMut();

/// Something an effect can be unsubscribed from by raw pointer identity.
/// Lets `EffectNode` hold a type-erased list of the signals it depends on,
/// without `Signal<T>` needing to know about effects of other `T`s.
trait Trackable {
    fn unsubscribe(&self, effect_ptr: *const EffectNode);
}

impl<T> Trackable for Rc<RefCell<SignalInner<T>>> {
    fn unsubscribe(&self, effect_ptr: *const EffectNode) {
        self.borrow_mut()
            .subscribers
            .retain(|weak| weak.as_ptr() != effect_ptr);
    }
}

struct EffectNode {
    f: RefCell<Box<EffectFn>>,
    /// Signals read during the most recent run, kept so they can be
    /// unsubscribed before the next run recomputes dependencies from scratch.
    deps: RefCell<Vec<Rc<dyn Trackable>>>,
}

thread_local! {
    static EFFECT_STACK: RefCell<Vec<Rc<EffectNode>>> = const { RefCell::new(Vec::new()) };
}

// ---------------------------------------------------------------------------
// Hydration state — set before creating signals so `Signal::hydrated` can
// restore server-side values instead of using the default.
// ---------------------------------------------------------------------------

thread_local! {
    static HYDRATION_STATE: RefCell<Option<HashMap<String, serde_json::Value>>> =
        const { RefCell::new(None) };
}

/// Feed server-serialised signal values into the runtime before any
/// `Signal::hydrated(...)` call. The map is consumed once (cleared on read).
pub fn set_hydration_state(state: HashMap<String, serde_json::Value>) {
    HYDRATION_STATE.with(|s| *s.borrow_mut() = Some(state));
}

/// Take (and clear) the current hydration state, if any.
pub fn take_hydration_state() -> Option<HashMap<String, serde_json::Value>> {
    HYDRATION_STATE.with(|s| s.borrow_mut().take())
}

/// A reactive value. Cloning a `Signal` gives another handle to the same
/// underlying storage (like `Rc`), not an independent copy of the value.
pub struct Signal<T> {
    inner: Rc<RefCell<SignalInner<T>>>,
}

struct SignalInner<T> {
    value: T,
    subscribers: Vec<Weak<EffectNode>>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Signal {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: Clone + 'static> Signal<T> {
    pub fn new(value: T) -> Self {
        Signal {
            inner: Rc::new(RefCell::new(SignalInner {
                value,
                subscribers: Vec::new(),
            })),
        }
    }

    /// Create a signal whose initial value is taken from the server-side
    /// hydration state (keyed by `name`). Falls back to `default` when no
    /// hydration data is available — making it safe to use in both SSR and
    /// client-only contexts.
    pub fn hydrated(name: &str, default: T) -> Self
    where
        T: DeserializeOwned,
    {
        let value = HYDRATION_STATE.with(|s| {
            s.borrow()
                .as_ref()
                .and_then(|m| m.get(name).cloned())
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or(default)
        });
        Signal::new(value)
    }

    /// Reads the current value, subscribing the currently-running effect
    /// (if any) to future updates of this signal.
    pub fn get(&self) -> T {
        EFFECT_STACK.with(|stack| {
            if let Some(node) = stack.borrow().last() {
                let already_subscribed = self
                    .inner
                    .borrow()
                    .subscribers
                    .iter()
                    .any(|weak| weak.as_ptr() == Rc::as_ptr(node));
                if !already_subscribed {
                    self.inner
                        .borrow_mut()
                        .subscribers
                        .push(Rc::downgrade(node));
                    node.deps
                        .borrow_mut()
                        .push(Rc::new(Rc::clone(&self.inner)));
                }
            }
        });
        self.inner.borrow().value.clone()
    }

    /// Updates the value and synchronously re-runs every effect that has
    /// read this signal (and is still alive).
    pub fn set(&self, value: T) {
        self.inner.borrow_mut().value = value;
        self.notify();
    }

    fn notify(&self) {
        let effects: Vec<Rc<EffectNode>> = {
            let mut inner = self.inner.borrow_mut();
            inner.subscribers.retain(|weak| weak.strong_count() > 0);
            inner.subscribers.iter().filter_map(Weak::upgrade).collect()
        };
        for effect in effects {
            run_effect(&effect);
        }
    }
}

/// A handle to a running effect. Dropping it unsubscribes the effect from
/// every signal it depends on (since subscribers are held as `Weak`).
#[must_use = "dropping an EffectHandle stops the effect from re-running"]
pub struct EffectHandle {
    _inner: Rc<EffectNode>,
}

/// Runs `f` immediately, then re-runs it whenever any `Signal` it read
/// during that run is updated. Returns a handle that must be kept alive
/// for as long as the effect should keep reacting.
pub fn create_effect(f: impl FnMut() + 'static) -> EffectHandle {
    let node = Rc::new(EffectNode {
        f: RefCell::new(Box::new(f)),
        deps: RefCell::new(Vec::new()),
    });
    run_effect(&node);
    EffectHandle { _inner: node }
}

fn run_effect(node: &Rc<EffectNode>) {
    let stale_deps = node.deps.replace(Vec::new());
    let effect_ptr: *const EffectNode = Rc::as_ptr(node);
    for dep in &stale_deps {
        dep.unsubscribe(effect_ptr);
    }

    EFFECT_STACK.with(|stack| stack.borrow_mut().push(Rc::clone(node)));
    (node.f.borrow_mut())();
    EFFECT_STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn get_returns_current_value() {
        let s = Signal::new(1);
        assert_eq!(s.get(), 1);
        s.set(2);
        assert_eq!(s.get(), 2);
    }

    #[test]
    fn effect_reruns_only_for_signals_it_reads() {
        let a = Signal::new(1);
        let b = Signal::new(10);

        let a_runs = Rc::new(Cell::new(0));
        let a_runs_clone = Rc::clone(&a_runs);
        let a_for_effect = a.clone();
        let _handle = create_effect(move || {
            a_for_effect.get();
            a_runs_clone.set(a_runs_clone.get() + 1);
        });

        assert_eq!(a_runs.get(), 1, "effect runs once immediately");

        b.set(20);
        assert_eq!(a_runs.get(), 1, "unrelated signal must not trigger rerun");

        a.set(2);
        assert_eq!(a_runs.get(), 2, "dependency update must trigger rerun");
    }

    #[test]
    fn dropping_handle_stops_reruns() {
        let a = Signal::new(1);
        let runs = Rc::new(Cell::new(0));
        let runs_clone = Rc::clone(&runs);
        let a_for_effect = a.clone();
        let handle = create_effect(move || {
            a_for_effect.get();
            runs_clone.set(runs_clone.get() + 1);
        });

        drop(handle);
        a.set(2);
        assert_eq!(
            runs.get(),
            1,
            "effect must not rerun after its handle is dropped"
        );
    }

    #[test]
    fn dependencies_can_change_between_runs() {
        let cond = Signal::new(true);
        let a = Signal::new(1);
        let b = Signal::new(100);

        let seen = Rc::new(RefCell::new(Vec::new()));
        let seen_clone = Rc::clone(&seen);
        let cond_e = cond.clone();
        let a_e = a.clone();
        let b_e = b.clone();
        let _handle = create_effect(move || {
            let value = if cond_e.get() { a_e.get() } else { b_e.get() };
            seen_clone.borrow_mut().push(value);
        });

        assert_eq!(*seen.borrow(), vec![1]);

        // Still on the `a` branch; `b` must not trigger a rerun yet.
        b.set(200);
        assert_eq!(*seen.borrow(), vec![1]);

        cond.set(false);
        assert_eq!(
            *seen.borrow(),
            vec![1, 200],
            "switching branches re-subscribes to b"
        );

        a.set(2);
        assert_eq!(
            *seen.borrow(),
            vec![1, 200],
            "no longer depends on a after branch switch"
        );

        b.set(300);
        assert_eq!(*seen.borrow(), vec![1, 200, 300]);
    }

    #[test]
    fn hydrated_uses_hydration_state_when_available() {
        let mut state = std::collections::HashMap::new();
        state.insert("count".to_string(), serde_json::json!(42));
        super::set_hydration_state(state);

        let s: Signal<i32> = Signal::hydrated("count", 0);
        assert_eq!(s.get(), 42, "should pick up the server-side value");

        // State is still available (not consumed on read) for subsequent calls.
        let s2: Signal<i32> = Signal::hydrated("count", 0);
        assert_eq!(s2.get(), 42, "state is not consumed until take_hydration_state");

        // After explicit take, new signals fall back to default.
        let _taken = super::take_hydration_state();
        let s3: Signal<i32> = Signal::hydrated("count", 0);
        assert_eq!(s3.get(), 0, "hydration state was consumed by take");
    }

    #[test]
    fn hydrated_falls_back_to_default_without_hydration_state() {
        let s: Signal<i32> = Signal::hydrated("missing", 99);
        assert_eq!(s.get(), 99);
    }
}
