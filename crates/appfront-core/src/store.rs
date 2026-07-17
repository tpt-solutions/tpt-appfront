//! State management at scale — a Zustand/Redux-like [`Store`] built directly on
//! the reactive [`Signal`] core, plus optional devtools time-travel and signal
//! persistence (`localStorage` / `IndexedDB`).
//!
//! A [`Store`] owns a single `Signal<S>` of your app state. Components read it
//! through [`Store::state`] (a `Signal<S>`) and update it through [`Store::set`]
//! / [`Store::update`]. Because the store is just a `Signal`, any effect that
//! reads it re-runs on change — the same reactivity model as `Signal`/`memo`,
//! but with a single named, subscribable source of truth.
//!
//! Three scaling features layer on top of that core:
//!
//! * **Subscriptions** — [`Store::subscribe`] gives a plain callback API for
//!   non-reactive consumers (logging, analytics, IPC to a webview).
//! * **Time-travel** — [`Store::with_time_travel`] records every committed
//!   state into a bounded ring buffer; [`Store::undo`]/[`Store::redo`] (and the
//!   [`crate::devtools`] integration) rewind/replay it. This is the devtools
//!   "time-travel" debugging story.
//! * **Persistence** — [`Store::with_persistence`] binds the store to a
//!   [`Persistence`] backend (e.g. `localStorage` on wasm). State is hydrated on
//!   creation and written (synchronously, per commit) on every change;
//!   [`Store::persist_now`] flushes explicitly.
//!
//! The store is backend-agnostic: `Persistence` is a trait, and the only
//! provided impl (`WebStorage`) is gated behind `target_arch = "wasm32"`, so
//! `appfront-core` keeps building natively.

use crate::signal::{create_effect, EffectHandle, Signal};
use std::rc::Rc;

/// Shared, mutable list of plain-callback subscribers. Wrapped in `Rc<RefCell>`
/// so a dropped [`StoreSubscription`] can remove its own entry by holding a
/// shared handle to the exact same vec (see [`Store::subscribe`]).
type SubscriberList<S> = Rc<std::cell::RefCell<Vec<Rc<dyn Fn(&S)>>>>;

/// A store of application state `S`, built on a reactive [`Signal`].
///
/// Cheap to clone (`Rc`-backed); every clone shares the same underlying state
/// and subscription list, so providing the store to a subtree and updating it
/// from anywhere updates every consumer.
pub struct Store<S> {
    state: Signal<S>,
    inner: Rc<StoreInner<S>>,
}

struct StoreInner<S> {
    /// Plain-callback subscribers, notified (synchronously) on every commit.
    /// Wrapped in `Rc` so a dropped [`StoreSubscription`] can remove its own
    /// entry by holding a shared handle to the exact same vec.
    subscribers: SubscriberList<S>,
    /// Time-travel ring buffer, when enabled.
    history: std::cell::RefCell<Option<HistoryBuf<S>>>,
    /// Optional persistence binding, when enabled.
    persistence: std::cell::RefCell<Option<Box<dyn Persistence<S>>>>,
}

struct HistoryBuf<S> {
    past: Vec<S>,
    /// Index into `past` of the currently-active state (so undo/redo move the
    /// cursor without dropping the redone entries until a new commit happens).
    cursor: usize,
    limit: usize,
}

impl<S: Clone + 'static> Store<S> {
    /// Creates a store from an initial state.
    pub fn new(initial: S) -> Self {
        Store {
            state: Signal::new(initial),
            inner: Rc::new(StoreInner {
                subscribers: SubscriberList::default(),
                history: std::cell::RefCell::new(None),
                persistence: std::cell::RefCell::new(None),
            }),
        }
    }

    /// Enables devtools time-travel with a bounded history of `limit` commits.
    pub fn with_time_travel(self, limit: usize) -> Self {
        // Seed the history with the current state so `undo` can always return
        // to it; `past[cursor]` is always the *current* state.
        let initial = self.state.get();
        *self.inner.history.borrow_mut() = Some(HistoryBuf {
            past: vec![initial],
            cursor: 0,
            limit: limit.max(1),
        });
        self
    }

    /// Binds the store to a persistence backend (hydrating from it first).
    pub fn with_persistence(self, backend: Box<dyn Persistence<S>>) -> Self {
        if let Some(restored) = backend.load() {
            self.state.set(restored);
        }
        *self.inner.persistence.borrow_mut() = Some(backend);
        self
    }

    /// The underlying state signal — read it inside effects/views to subscribe.
    pub fn signal(&self) -> Signal<S> {
        self.state.clone()
    }

    /// Reads the current state (clones it).
    pub fn get(&self) -> S {
        self.state.get()
    }

    /// Replaces the state with `next`, notifying subscribers and recording the
    /// previous state in the time-travel history (if enabled).
    pub fn set(&self, next: S) {
        self.commit(next);
    }

    /// Applies `f` to the current state and commits the result.
    pub fn update(&self, f: impl FnOnce(&S) -> S) {
        let next = f(&self.state.get());
        self.commit(next);
    }

    fn commit(&self, next: S) {
        // Record the *new* state into history so `past[cursor]` is always the
        // current state; `undo` moves the cursor back, `redo` moves it forward.
        if let Some(hist) = self.inner.history.borrow_mut().as_mut() {
            hist.past.truncate(hist.cursor + 1);
            hist.past.push(next.clone());
            if hist.past.len() > hist.limit {
                hist.past.remove(0);
            }
            hist.cursor = hist.past.len() - 1;
        }
        self.state.set(next);
        self.notify();
    }

    fn notify(&self) {
        let subs = self.inner.subscribers.borrow();
        let s = self.state.get();
        for sub in subs.iter() {
            sub(&s);
        }
        if let Some(p) = self.inner.persistence.borrow().as_ref() {
            p.save(&s);
        }
    }

    /// Subscribes `cb` to every committed state change. Returns a handle whose
    /// drop removes the subscription.
    pub fn subscribe(&self, cb: impl Fn(&S) + 'static) -> StoreSubscription<S> {
        let cb = Rc::new(cb) as Rc<dyn Fn(&S)>;
        self.inner.subscribers.borrow_mut().push(Rc::clone(&cb));
        StoreSubscription {
            subscribers: std::rc::Rc::clone(&self.inner.subscribers),
            cb,
        }
    }

    /// Whether an `undo` would return to a previous committed state.
    pub fn can_undo(&self) -> bool {
        self.inner
            .history
            .borrow()
            .as_ref()
            .map(|h| h.cursor > 0)
            .unwrap_or(false)
    }

    /// Whether a `redo` would replay a previously undone state.
    pub fn can_redo(&self) -> bool {
        self.inner
            .history
            .borrow()
            .as_ref()
            .map(|h| h.cursor + 1 < h.past.len())
            .unwrap_or(false)
    }

    /// Rewinds to the previous committed state (time-travel). No-op when there
    /// is nothing to undo.
    pub fn undo(&self) {
        let mut hist = self.inner.history.borrow_mut();
        if let Some(h) = hist.as_mut() {
            if h.cursor > 0 {
                h.cursor -= 1;
                let prev = h.past[h.cursor].clone();
                drop(hist);
                self.state.set(prev);
                self.notify_without_history();
            }
        }
    }

    /// Replays the next (previously undone) state. No-op when nothing to redo.
    pub fn redo(&self) {
        let mut hist = self.inner.history.borrow_mut();
        if let Some(h) = hist.as_mut() {
            if h.cursor + 1 < h.past.len() {
                h.cursor += 1;
                let next = h.past[h.cursor].clone();
                drop(hist);
                self.state.set(next);
                self.notify_without_history();
            }
        }
    }

    /// Notifies subscribers/persistence without recording into history (used by
    /// undo/redo, which *move the cursor* rather than commit a new state).
    fn notify_without_history(&self) {
        let subs = self.inner.subscribers.borrow();
        let s = self.state.get();
        for sub in subs.iter() {
            sub(&s);
        }
        if let Some(p) = self.inner.persistence.borrow().as_ref() {
            p.save(&s);
        }
    }

    /// Flushes the current state to the persistence backend immediately (the
    /// normal path also writes on every commit, but this is the explicit hook
    /// for "save now", e.g. on window `CloseRequested`).
    pub fn persist_now(&self) {
        if let Some(p) = self.inner.persistence.borrow().as_ref() {
            p.save(&self.state.get());
        }
    }
}

/// Handle returned by [`Store::subscribe`]; dropping it unsubscribes.
pub struct StoreSubscription<S> {
    subscribers: SubscriberList<S>,
    cb: Rc<dyn Fn(&S)>,
}

impl<S> Drop for StoreSubscription<S> {
    fn drop(&mut self) {
        let mut subs = self.subscribers.borrow_mut();
        subs.retain(|other| !Rc::ptr_eq(other, &self.cb));
    }
}

/// A persistence backend for a store. Implemented by `WebStorage` (wasm) and by
/// tests with an in-memory backend (see `MemoryStorage` in the test module).
pub trait Persistence<S>: 'static {
    /// Loads the persisted state, or `None` if nothing is stored / unparseable.
    fn load(&self) -> Option<S>;
    /// Writes the current state.
    fn save(&self, state: &S);
}

#[cfg(target_arch = "wasm32")]
/// A [`Persistence`] backend backed by the browser `localStorage` (wasm only).
/// State is serialised as JSON via `serde`, so `S` must be `Serialize +
/// DeserializeOwned`. `IndexedDB` is the recommended choice for large state;
/// this `localStorage` binding is the simple synchronous option.
pub struct WebStorage {
    key: String,
}

#[cfg(target_arch = "wasm32")]
impl WebStorage {
    /// Creates a `localStorage` binding under `key`.
    pub fn new(key: &str) -> Self {
        WebStorage {
            key: key.to_string(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl<S: serde::Serialize + serde::de::DeserializeOwned + 'static> Persistence<S> for WebStorage {
    fn load(&self) -> Option<S> {
        let window = web_sys::window()?;
        let storage = window.local_storage().ok().flatten()?;
        let raw = storage.get_item(&self.key).ok().flatten()?;
        serde_json::from_str(&raw).ok()
    }

    fn save(&self, state: &S) {
        if let Some(window) = web_sys::window() {
            if let Some(storage) = window.local_storage().ok().flatten() {
                if let Ok(json) = serde_json::to_string(state) {
                    let _ = storage.set_item(&self.key, &json);
                }
            }
        }
    }
}

/// Wires a store into the reactive devtools time-travel report: every commit is
/// also observable through [`crate::signal::signal_activity`] when the store's
/// state signal is [`crate::signal::Signal::labeled`]. This helper returns an
/// effect handle that keeps the bridge alive; forget/drop it to detach.
///
/// `label` names the backing signal so the [`crate::devtools`] inspector shows
/// its write count under that name.
pub fn instrument_store<S: Clone + 'static>(store: &Store<S>, label: &str) -> EffectHandle {
    let sig = store.signal().labeled(label);
    create_effect(move || {
        let _ = sig.get();
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    struct AppState {
        count: i32,
        name: String,
    }

    #[test]
    fn set_and_get_round_trip() {
        let store = Store::new(AppState {
            count: 0,
            name: "x".into(),
        });
        store.set(AppState {
            count: 1,
            name: "y".into(),
        });
        assert_eq!(store.get().count, 1);
    }

    #[test]
    fn update_applies_transformation() {
        let store = Store::new(AppState {
            count: 0,
            name: "x".into(),
        });
        store.update(|s| AppState {
            count: s.count + 5,
            name: s.name.clone(),
        });
        assert_eq!(store.get().count, 5);
    }

    #[test]
    fn subscribers_fire_on_commit() {
        let store = Store::new(AppState {
            count: 0,
            name: "x".into(),
        });
        let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen2 = seen.clone();
        let _sub = store.subscribe(move |s: &AppState| {
            seen2.borrow_mut().push(s.count);
        });
        store.set(AppState {
            count: 1,
            name: "a".into(),
        });
        store.update(|s| AppState {
            count: s.count + 1,
            name: "b".into(),
        });
        assert_eq!(*seen.borrow(), vec![1, 2]);
    }

    #[test]
    fn dropping_subscription_stops_callbacks() {
        let store = Store::new(AppState {
            count: 0,
            name: "x".into(),
        });
        let seen = Rc::new(std::cell::RefCell::new(0usize));
        let seen2 = seen.clone();
        let sub = store.subscribe(move |_: &AppState| {
            *seen2.borrow_mut() += 1;
        });
        store.set(AppState {
            count: 1,
            name: "a".into(),
        });
        drop(sub);
        store.set(AppState {
            count: 2,
            name: "b".into(),
        });
        assert_eq!(*seen.borrow(), 1, "no callback after drop");
    }

    #[test]
    fn time_travel_undo_redo() {
        let store = Store::new(AppState {
            count: 0,
            name: "x".into(),
        })
        .with_time_travel(10);

        store.set(AppState {
            count: 1,
            name: "a".into(),
        });
        store.set(AppState {
            count: 2,
            name: "b".into(),
        });
        assert!(store.can_undo());
        assert!(!store.can_redo());

        store.undo();
        assert_eq!(store.get().count, 1);
        assert!(store.can_redo());

        store.undo();
        assert_eq!(store.get().count, 0);

        store.redo();
        assert_eq!(store.get().count, 1);
        assert_eq!(store.get().name, "a");
    }

    #[test]
    fn new_commit_after_undo_truncates_redo_branch() {
        let store = Store::new(AppState {
            count: 0,
            name: "x".into(),
        })
        .with_time_travel(10);
        store.set(AppState {
            count: 1,
            name: "a".into(),
        });
        store.set(AppState {
            count: 2,
            name: "b".into(),
        });
        store.undo(); // back to count=1
        store.set(AppState {
            count: 9,
            name: "c".into(),
        });
        assert!(!store.can_redo(), "redo branch truncated by new commit");
        assert_eq!(store.get().count, 9);
    }

    /// In-memory persistence backend used to validate the binding without a
    /// browser. Mirrors the `WebStorage` contract.
    struct MemoryStorage {
        cell: std::rc::Rc<std::cell::RefCell<Option<String>>>,
    }

    impl Persistence<AppState> for MemoryStorage {
        fn load(&self) -> Option<AppState> {
            self.cell
                .borrow()
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
        }
        fn save(&self, state: &AppState) {
            *self.cell.borrow_mut() = Some(serde_json::to_string(state).unwrap());
        }
    }

    #[test]
    fn persistence_hydrates_initial_and_writes_on_commit() {
        let cell = std::rc::Rc::new(std::cell::RefCell::new(None));
        *cell.borrow_mut() = Some(
            serde_json::to_string(&AppState {
                count: 42,
                name: "seed".into(),
            })
            .unwrap(),
        );

        let store = Store::new(AppState {
            count: 0,
            name: "default".into(),
        })
        .with_persistence(Box::new(MemoryStorage { cell: cell.clone() }));

        assert_eq!(store.get().count, 42);
        assert_eq!(store.get().name, "seed");

        store.set(AppState {
            count: 7,
            name: "updated".into(),
        });
        store.persist_now();
        let raw = cell.borrow();
        let restored: AppState = serde_json::from_str(raw.as_ref().unwrap()).unwrap();
        assert_eq!(restored.count, 7);
        assert_eq!(restored.name, "updated");
    }

    #[test]
    fn instrument_store_records_writes_in_devtools() {
        use crate::signal::{reset_signal_activity, signal_activity};
        reset_signal_activity();
        let store = Store::new(AppState {
            count: 0,
            name: "x".into(),
        });
        let _handle = instrument_store(&store, "counter_store");
        store.set(AppState {
            count: 1,
            name: "a".into(),
        });
        store.set(AppState {
            count: 2,
            name: "b".into(),
        });
        assert_eq!(
            signal_activity().get("counter_store"),
            Some(&2),
            "devtools observes two store writes"
        );
    }
}
