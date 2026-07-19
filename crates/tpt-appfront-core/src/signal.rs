//! A minimal SolidJS-style reactive signal system.
//!
//! `Signal::get` records itself as a dependency of whichever `Effect` is
//! currently running (tracked via a thread-local stack), so `Signal::set`
//! only re-runs the effects that actually read that signal — no diffing.
//! Dependencies are recomputed on every run (not just accumulated), so an
//! effect that branches (`if cond.get() { a.get() } else { b.get() }`)
//! only stays subscribed to whichever branch it last took.

use serde::de::DeserializeOwned;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
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
    /// Scheduling order for batched flushes: 0 for an effect that only reads
    /// plain signals, or `1 + max(rank of any dependency's producing effect)`
    /// for an effect (e.g. a memo) downstream of other memos. Reset to 0 at
    /// the start of every run and recomputed as `get()` is called, so it
    /// self-corrects when an effect's dependencies change between runs.
    rank: Cell<u32>,
}

thread_local! {
    static EFFECT_STACK: RefCell<Vec<Rc<EffectNode>>> = const { RefCell::new(Vec::new()) };
}

// ---------------------------------------------------------------------------
// Batched/topological flush scheduling.
//
// `notify()` always enqueues affected effects here instead of running them
// directly. Outside an explicit `batch()` call the queue is flushed inline
// (so `set()` remains synchronous by default, matching prior behavior);
// inside `batch()`, flushing is deferred until the outermost call returns.
// Flushing drains the queue in ascending `rank` order so producers (e.g. a
// memo `B`) run before their consumers (e.g. a memo `D` reading `B`), and
// loops until the queue is empty so second-order fan-in (an effect enqueued
// by another effect's own run, e.g. `D` becoming pending only once both `B`
// and `C` have notified) is still picked up in the same flush.
// ---------------------------------------------------------------------------

thread_local! {
    static BATCH_DEPTH: Cell<u32> = const { Cell::new(0) };
    static PENDING: RefCell<Vec<Weak<EffectNode>>> = const { RefCell::new(Vec::new()) };
    static PENDING_PTRS: RefCell<HashSet<*const EffectNode>> = RefCell::new(HashSet::new());
}

fn enqueue(effects: impl IntoIterator<Item = Rc<EffectNode>>) {
    PENDING_PTRS.with(|ptrs| {
        PENDING.with(|pending| {
            let mut ptrs = ptrs.borrow_mut();
            let mut pending = pending.borrow_mut();
            for effect in effects {
                let ptr: *const EffectNode = Rc::as_ptr(&effect);
                if ptrs.insert(ptr) {
                    pending.push(Rc::downgrade(&effect));
                }
            }
        });
    });
}

fn flush() {
    loop {
        let batch: Vec<Rc<EffectNode>> = PENDING.with(|pending| {
            let mut pending = pending.borrow_mut();
            let mut batch: Vec<Rc<EffectNode>> =
                std::mem::take(&mut *pending).into_iter().filter_map(|w| w.upgrade()).collect();
            batch.sort_by_key(|e| e.rank.get());
            batch
        });
        PENDING_PTRS.with(|ptrs| ptrs.borrow_mut().clear());
        if batch.is_empty() {
            break;
        }
        // Guard against reentrancy: running an effect in this pass may itself
        // call `notify()` (e.g. a memo recomputing and writing its signal).
        // Without this, that nested `notify()` would see `BATCH_DEPTH == 0`
        // and immediately recurse into `flush()`, running a downstream
        // consumer (e.g. `D`) before a sibling producer in *this* pass (e.g.
        // `C`) has had a chance to run. Bumping the depth makes nested
        // `notify()` calls just enqueue; the outer `loop` picks up whatever
        // they enqueued as the next pass, once this whole pass has settled.
        BATCH_DEPTH.with(|d| d.set(d.get() + 1));
        for effect in batch {
            run_effect(&effect);
        }
        BATCH_DEPTH.with(|d| d.set(d.get() - 1));
    }
}

/// Defers effect execution during `f` and, once the outermost `batch()` call
/// returns, runs every affected effect at most once, in dependency order.
/// Nested `batch()` calls only flush when the outermost one exits.
pub fn batch(f: impl FnOnce()) {
    BATCH_DEPTH.with(|d| d.set(d.get() + 1));
    f();
    let depth = BATCH_DEPTH.with(|d| {
        let new_depth = d.get() - 1;
        d.set(new_depth);
        new_depth
    });
    if depth == 0 {
        flush();
    }
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
    /// Set only for signals created by `create_memo`: the internal effect
    /// that recomputes `value`. Kept alive here so it keeps reacting for as
    /// long as the returned `Signal` handle (or a clone of it) is alive, and
    /// consulted by `get()` to rank downstream consumers correctly.
    memo_effect: Option<Rc<EffectNode>>,
    /// Optional human-readable name used by the devtools inspector to report
    /// how often this signal is written. `None` (the default) means the signal
    /// is anonymous and never contributes to the activity log.
    label: Option<String>,
}

// ---------------------------------------------------------------------------
// Optional signal-write activity tracking (devtools inspector).
//
// Cheap by construction: only signals explicitly named via `Signal::labeled`
// ever touch this map, so anonymous signals pay nothing extra on `set()`.
// ---------------------------------------------------------------------------

thread_local! {
    static SIGNAL_ACTIVITY: RefCell<HashMap<String, u64>> = RefCell::new(HashMap::new());
}

/// Snapshot of how many times each labeled signal has been written since the
/// last [`reset_signal_activity`], used by the devtools inspector to show
/// "which signals are firing". Only signals named via [`Signal::labeled`]
/// appear here.
pub fn signal_activity() -> HashMap<String, u64> {
    SIGNAL_ACTIVITY.with(|a| a.borrow().clone())
}

/// Clears the per-label signal-write counters returned by [`signal_activity`].
pub fn reset_signal_activity() {
    SIGNAL_ACTIVITY.with(|a| a.borrow_mut().clear());
}

fn record_signal_write(label: &str) {
    SIGNAL_ACTIVITY.with(|a| *a.borrow_mut().entry(label.to_string()).or_insert(0) += 1);
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
                memo_effect: None,
                label: None,
            })),
        }
    }

    /// Attaches a human-readable `name` to this signal so the devtools
    /// inspector can report how often it is written (see
    /// [`signal_activity`]). Purely a debugging aid — it changes nothing
    /// about reactivity. Returns `self` for chaining.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = Signal::new(0i32).labeled("count");
    /// // ... after some updates ...
    /// assert_eq!(signal_activity().get("count"), Some(&3));
    /// ```
    pub fn labeled(self, name: &str) -> Self {
        self.inner.borrow_mut().label = Some(name.to_string());
        self
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

    /// Subscribes the currently-running effect (if any) to this signal:
    /// updates its rank so downstream memos flush in the right order, and
    /// registers it as a subscriber if not already. Shared by `get`/`with`.
    fn track(&self) {
        EFFECT_STACK.with(|stack| {
            if let Some(node) = stack.borrow().last() {
                // A signal backed by a memo ranks its consumers above itself
                // so a downstream memo always flushes after this one.
                let producing_rank = self
                    .inner
                    .borrow()
                    .memo_effect
                    .as_ref()
                    .map(|e| e.rank.get() + 1)
                    .unwrap_or(0);
                if producing_rank > node.rank.get() {
                    node.rank.set(producing_rank);
                }

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
    }

    /// Reads the current value, subscribing the currently-running effect
    /// (if any) to future updates of this signal.
    pub fn get(&self) -> T {
        self.track();
        self.inner.borrow().value.clone()
    }

    /// Reads the current value via a borrow instead of cloning it, still
    /// subscribing the currently-running effect. Prefer this over `get()`
    /// for large values (e.g. a `DataGrid`'s row vector) where the mandatory
    /// clone on every read would be wasteful.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.track();
        f(&self.inner.borrow().value)
    }

    /// Updates the value and, by default (outside a `batch()` call),
    /// synchronously re-runs every effect that has read this signal (and is
    /// still alive) before returning. Inside `batch()`, affected effects are
    /// deferred and deduped until the outermost `batch()` call exits.
    pub fn set(&self, value: T) {
        self.inner.borrow_mut().value = value;
        self.notify();
    }

    fn notify(&self) {
        let label = self.inner.borrow().label.clone();
        let effects: Vec<Rc<EffectNode>> = {
            let mut inner = self.inner.borrow_mut();
            inner.subscribers.retain(|weak| weak.strong_count() > 0);
            inner.subscribers.iter().filter_map(Weak::upgrade).collect()
        };
        enqueue(effects);
        if BATCH_DEPTH.with(Cell::get) == 0 {
            flush();
        }
        if let Some(label) = label {
            record_signal_write(&label);
        }
    }
}

impl<T: Clone + PartialEq + 'static> Signal<T> {
    /// Like `set`, but skips the write and notification entirely when
    /// `value` equals the current one. Used internally by `create_memo` so
    /// a memo whose recomputed value hasn't actually changed doesn't wake
    /// its own downstream subscribers.
    fn set_if_changed(&self, value: T) {
        let changed = self.inner.borrow().value != value;
        if changed {
            self.set(value);
        }
    }
}

/// Creates a read-derived `Signal` that recomputes `compute` whenever one of
/// the signals it reads changes, and only notifies its own subscribers when
/// the freshly computed value actually differs from the cached one. This
/// gives both caching (skip recompute-driven notifications for unchanged
/// output) and correct diamond-dependency ordering: a memo's internal effect
/// is ranked above the effects of any memo it depends on, so `flush()`
/// always settles producers before consumers.
pub fn create_memo<T: Clone + PartialEq + 'static>(compute: impl Fn() -> T + 'static) -> Signal<T> {
    // The memo's `Signal` can't exist until we know its initial value, but
    // computing that value (to track dependencies correctly) requires
    // running the effect. Bridge the two with a cell the closure writes its
    // very first result into, before the real signal exists.
    let first_value: Rc<RefCell<Option<T>>> = Rc::new(RefCell::new(None));
    let first_value_for_effect = Rc::clone(&first_value);
    let memo_signal_cell: Rc<RefCell<Option<Signal<T>>>> = Rc::new(RefCell::new(None));
    let memo_signal_for_effect = Rc::clone(&memo_signal_cell);

    let node = Rc::new(EffectNode {
        f: RefCell::new(Box::new(move || {
            let value = compute();
            match memo_signal_for_effect.borrow().as_ref() {
                Some(signal) => signal.set_if_changed(value),
                None => *first_value_for_effect.borrow_mut() = Some(value),
            }
        })),
        deps: RefCell::new(Vec::new()),
        rank: Cell::new(0),
    });

    // First run: computes exactly once and tracks dependencies via `get()`.
    run_effect(&node);
    let initial = first_value
        .borrow_mut()
        .take()
        .expect("create_memo's compute closure must run synchronously");

    let memo_signal = Signal::new(initial);
    memo_signal.inner.borrow_mut().memo_effect = Some(Rc::clone(&node));
    *memo_signal_cell.borrow_mut() = Some(memo_signal.clone());
    memo_signal
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
        rank: Cell::new(0),
    });
    run_effect(&node);
    EffectHandle { _inner: node }
}

fn run_effect(node: &Rc<EffectNode>) {
    node.rank.set(0);
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

    #[test]
    fn with_reads_without_cloning_and_still_tracks() {
        let s = Signal::new(vec![1, 2, 3]);
        assert_eq!(s.with(|v| v.len()), 3);

        let runs = Rc::new(Cell::new(0));
        let runs_clone = Rc::clone(&runs);
        let s_for_effect = s.clone();
        let _handle = create_effect(move || {
            s_for_effect.with(|v| v.len());
            runs_clone.set(runs_clone.get() + 1);
        });
        assert_eq!(runs.get(), 1);

        s.set(vec![1, 2, 3, 4]);
        assert_eq!(runs.get(), 2, "with() must subscribe like get() does");
    }

    #[test]
    fn memo_recomputes_only_when_dependency_changes() {
        let a = Signal::new(1);
        let b = Signal::new(10);

        let computes = Rc::new(Cell::new(0));
        let computes_clone = Rc::clone(&computes);
        let a_for_memo = a.clone();
        let m = create_memo(move || {
            computes_clone.set(computes_clone.get() + 1);
            a_for_memo.get() * 2
        });

        assert_eq!(computes.get(), 1, "compute runs once on creation");
        assert_eq!(m.get(), 2);

        b.set(20);
        assert_eq!(computes.get(), 1, "unrelated signal must not trigger recompute");

        a.set(5);
        assert_eq!(computes.get(), 2, "dependency update must trigger recompute");
        assert_eq!(m.get(), 10);
    }

    #[test]
    fn memo_skips_notify_when_value_unchanged() {
        let a = Signal::new(1i32);
        let a_for_memo = a.clone();
        let m = create_memo(move || a_for_memo.get().abs());

        let downstream_runs = Rc::new(Cell::new(0));
        let downstream_runs_clone = Rc::clone(&downstream_runs);
        let m_for_effect = m.clone();
        let _handle = create_effect(move || {
            m_for_effect.get();
            downstream_runs_clone.set(downstream_runs_clone.get() + 1);
        });

        assert_eq!(downstream_runs.get(), 1, "effect runs once immediately");

        a.set(-1);
        assert_eq!(
            downstream_runs.get(),
            1,
            "memo's abs() value is unchanged (1 -> 1), downstream must not rerun"
        );

        a.set(-2);
        assert_eq!(
            downstream_runs.get(),
            2,
            "memo's value actually changed (1 -> 2), downstream must rerun"
        );
    }

    #[test]
    fn diamond_d_runs_exactly_once() {
        let a = Signal::new(1);

        let a_for_b = a.clone();
        let b = create_memo(move || a_for_b.get() * 2);

        let a_for_c = a.clone();
        let c = create_memo(move || a_for_c.get() + 1);

        let d_computes = Rc::new(Cell::new(0));
        let d_computes_clone = Rc::clone(&d_computes);
        let b_for_d = b.clone();
        let c_for_d = c.clone();
        let d = create_memo(move || {
            d_computes_clone.set(d_computes_clone.get() + 1);
            b_for_d.get() + c_for_d.get()
        });

        assert_eq!(d_computes.get(), 1, "D computes once on creation");
        assert_eq!(d.get(), 4, "initial: b=2, c=2, d=4");

        a.set(10);

        assert_eq!(
            d_computes.get(),
            2,
            "D must recompute exactly once after A settles B and C, not zero or twice"
        );
        assert_eq!(d.get(), 31, "D must reflect both updated B (20) and C (11)");
    }

    #[test]
    fn batch_defers_and_dedupes_across_multiple_sets() {
        let x = Signal::new(1);
        let y = Signal::new(2);

        let runs = Rc::new(Cell::new(0));
        let runs_clone = Rc::clone(&runs);
        let x_for_effect = x.clone();
        let y_for_effect = y.clone();
        let _handle = create_effect(move || {
            x_for_effect.get();
            y_for_effect.get();
            runs_clone.set(runs_clone.get() + 1);
        });

        assert_eq!(runs.get(), 1, "effect runs once immediately");

        batch(|| {
            x.set(10);
            y.set(20);
        });
        assert_eq!(
            runs.get(),
            2,
            "batched writes to two deps the effect reads must cause exactly one rerun"
        );

        x.set(100);
        y.set(200);
        assert_eq!(
            runs.get(),
            4,
            "outside batch(), sequential sets still cause one rerun each"
        );
    }

    #[test]
    fn nested_batch_flushes_only_at_outermost_exit() {
        let s = Signal::new(1);
        let runs = Rc::new(Cell::new(0));
        let runs_clone = Rc::clone(&runs);
        let s_for_effect = s.clone();
        let _handle = create_effect(move || {
            s_for_effect.get();
            runs_clone.set(runs_clone.get() + 1);
        });

        assert_eq!(runs.get(), 1);

        batch(|| {
            batch(|| {
                s.set(2);
            });
            assert_eq!(runs.get(), 1, "inner batch exit must not flush yet");
            s.set(3);
        });

        assert_eq!(runs.get(), 2, "outer batch exit flushes exactly once total");
    }

    #[test]
    fn memo_rank_updates_after_dependency_change() {
        let cond = Signal::new(true);
        let a = Signal::new(1);
        let b = Signal::new(100);

        let cond_for_shallow = cond.clone();
        let a_for_shallow = a.clone();
        let b_for_shallow = b.clone();
        let shallow = create_memo(move || {
            if cond_for_shallow.get() {
                a_for_shallow.get()
            } else {
                b_for_shallow.get()
            }
        });

        let a_for_deep = a.clone();
        let deep_of_a = create_memo(move || a_for_deep.get() * 10);

        let seen = Rc::new(RefCell::new(Vec::new()));
        let seen_clone = Rc::clone(&seen);
        let shallow_for_top = shallow.clone();
        let deep_for_top = deep_of_a.clone();
        let _handle = create_effect(move || {
            seen_clone
                .borrow_mut()
                .push(shallow_for_top.get() + deep_for_top.get());
        });

        assert_eq!(*seen.borrow(), vec![11], "1 + 1*10");

        // Switch off the `a` branch; `shallow` no longer depends on `a`, only
        // `deep_of_a` does. The top effect must still settle correctly.
        cond.set(false);
        assert_eq!(*seen.borrow(), vec![11, 110], "100 + 1*10");

        a.set(5);
        assert_eq!(
            *seen.borrow(),
            vec![11, 110, 150],
            "shallow no longer reacts to a; only deep_of_a updates (100 + 5*10)"
        );
    }
}
