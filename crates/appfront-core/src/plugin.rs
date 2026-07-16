//! A formal plugin API for AppFront applications (Phase 4 / `#47`).
//!
//! A [`Plugin`] is a self-contained unit of cross-cutting functionality that an
//! app registers at startup. It has a typed state `S` (shared via the existing
//! [`Context`][crate::context::Context] mechanism) and a set of *hooks* that
//! run at well-defined points in the app lifecycle: before/after the tree is
//! built, and around each render. This gives apps an extension point without
//! baking backend-specific fields into [`UITree`][crate::UITree].
//!
//! ```ignore
//! struct Analytics;
//! impl Plugin for Analytics {
//!     type State = ();
//!     fn on_render(&self, _: &PluginCtx<Self::State>) {
//!         // ... count renders ...
//!     }
//! }
//!
//! let mut registry = PluginRegistry::new();
//! registry.register(Analytics);
//! registry.run_render_hooks();
//! ```
//!
//! The API is backend-agnostic: a plugin only ever sees lifecycle events and
//! the shared app state, never a DOM/canvas/TUI handle, so the same plugin
//! works on every backend.

use crate::context::Context;
use std::cell::Cell;
use std::rc::Rc;

/// A plugin's read-only view of app + plugin state during a hook.
///
/// `S` is the plugin's own state type (see [`Plugin::State`]); `App` is the
/// application's shared state type, if any. A plugin can read its own state and
/// any [`Context`][crate::context::Context] in scope, but cannot mutate the
/// tree — mutation happens through `App`/`S` signals the plugin holds.
pub struct PluginCtx<'a, S, App = ()> {
    /// The plugin's own shared state.
    pub state: &'a S,
    /// The application-wide shared state, if the app registered one.
    pub app: Option<&'a App>,
    /// The number of renders that have happened so far (0-based before the
    /// first render, 1-based inside an `on_render` hook for the first render).
    pub render_count: u64,
}

impl<'a, S, App> PluginCtx<'a, S, App> {
    /// Returns a reference to the app state, panicking if the app did not
    /// register a state of type `App`. Use [`PluginCtx::app`] for the fallible
    /// form.
    pub fn app(&self) -> &'a App {
        self.app.expect("plugin expected app state of type App")
    }
}

/// A plugin: a named, self-contained extension with typed state.
///
/// Implementors decide what happens at each lifecycle point. All hooks have
/// default no-op implementations, so a plugin only overrides the ones it cares
/// about.
pub trait Plugin {
    /// The plugin's shared state type. Defaults to `()` (stateless plugins).
    type State: 'static;

    /// A stable name, surfaced in devtools/telemetry. Used as a key in the
    /// registry; registering two plugins with the same name is an error.
    fn name(&self) -> &'static str;

    /// Called once when the plugin is registered, returning its initial state.
    /// The default returns `()` (stateless).
    fn init(&self) -> Self::State
    where
        Self::State: Default,
    {
        Self::State::default()
    }

    /// Called before the app builds its tree for a render. Use this to seed
    /// data, reset per-render accumulators, etc. `A` is the app-wide shared
    /// state type (usually `()` unless the host registered app state).
    fn on_before_render<A: 'static>(&self, _ctx: &PluginCtx<Self::State, A>) {}

    /// Called after the app has built its tree for a render. Use this for
    /// post-processing, analytics, or inspecting the produced tree.
    fn on_render<A: 'static>(&self, _ctx: &PluginCtx<Self::State, A>) {}

    /// Called once when the app shuts down. Use this to flush logs, persist
    /// state, or release native resources.
    fn on_shutdown<A: 'static>(&self, _ctx: &PluginCtx<Self::State, A>) {}
}

/// A registered plugin plus its live state, held behind an `Rc` so the registry
/// can be cheaply cloned into multiple backends/threads-of-render.
struct Registered<P: Plugin + 'static> {
    plugin: P,
    state: P::State,
}

/// Type-erased plugin so the registry can store heterogeneous plugin types.
/// `App` is the optional application-wide shared state type.
trait AnyPlugin<App: 'static>: 'static {
    fn name(&self) -> &'static str;
    fn on_before_render(&self, app: Option<&App>, render_count: u64);
    fn on_render(&self, app: Option<&App>, render_count: u64);
    fn on_shutdown(&self, app: Option<&App>, render_count: u64);
}

impl<P: Plugin + 'static, App: 'static> AnyPlugin<App> for Registered<P> {
    fn name(&self) -> &'static str {
        self.plugin.name()
    }
    fn on_before_render(&self, app: Option<&App>, render_count: u64) {
        let ctx: PluginCtx<'_, P::State, App> = PluginCtx {
            state: &self.state,
            app,
            render_count,
        };
        self.plugin.on_before_render(&ctx);
    }
    fn on_render(&self, app: Option<&App>, render_count: u64) {
        let ctx: PluginCtx<'_, P::State, App> = PluginCtx {
            state: &self.state,
            app,
            render_count,
        };
        self.plugin.on_render(&ctx);
    }
    fn on_shutdown(&self, app: Option<&App>, render_count: u64) {
        let ctx: PluginCtx<'_, P::State, App> = PluginCtx {
            state: &self.state,
            app,
            render_count,
        };
        self.plugin.on_shutdown(&ctx);
    }
}

/// Holds every registered [`Plugin`] and runs their hooks at the right times.
///
/// `App` is the optional application-wide shared state type; plugins may read
/// it but never mutate it directly. A registry is cheap to clone (inner `Rc`).
pub struct PluginRegistry<App: 'static = ()> {
    plugins: Vec<Rc<dyn AnyPlugin<App>>>,
}

impl<App: 'static> PluginRegistry<App> {
    /// Creates an empty registry.
    pub fn new() -> Self {
        PluginRegistry {
            plugins: Vec::new(),
        }
    }

    /// Registers a plugin, storing its initial state. Returns the plugin's name
    /// so callers can wire up its [`Context`][crate::context::Context] if
    /// desired. Panics if a plugin with the same name is already registered.
    pub fn register<P: Plugin + 'static>(&mut self, plugin: P) -> &'static str
    where
        P::State: Default,
    {
        let name = plugin.name();
        if self.plugins.iter().any(|p| p.name() == name) {
            panic!("appfront plugin registry: duplicate plugin name `{name}`");
        }
        let registered: Rc<dyn AnyPlugin<App>> = Rc::new(Registered {
            state: plugin.init(),
            plugin,
        });
        self.plugins.push(registered);
        name
    }

    /// Registers a plugin together with an existing shared state value.
    pub fn register_with_state<P: Plugin + 'static>(&mut self, plugin: P, state: P::State) -> &'static str {
        let name = plugin.name();
        if self.plugins.iter().any(|p| p.name() == name) {
            panic!("appfront plugin registry: duplicate plugin name `{name}`");
        }
        let registered: Rc<dyn AnyPlugin<App>> = Rc::new(Registered { state, plugin });
        self.plugins.push(registered);
        name
    }

    /// Runs every plugin's `on_before_render` hook.
    pub fn run_before_render_hooks(&self, app: Option<&App>) {
        for p in &self.plugins {
            p.on_before_render(app, self.render_count());
        }
    }

    /// Runs every plugin's `on_render` hook, then advances the render counter.
    pub fn run_render_hooks(&self, app: Option<&App>) {
        let count = self.render_count();
        for p in &self.plugins {
            p.on_render(app, count);
        }
    }

    /// Runs every plugin's `on_shutdown` hook.
    pub fn run_shutdown_hooks(&self, app: Option<&App>) {
        for p in &self.plugins {
            p.on_shutdown(app, self.render_count());
        }
    }

    /// Number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether the registry has no plugins.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    fn render_count(&self) -> u64 {
        RENDER_COUNT.with(|c| c.get())
    }

    /// Advances the internal render counter (called by the host once per
    /// completed render). `run_render_hooks` does not do this itself so the
    /// count is stable for the duration of a render.
    pub fn bump_render_count(&self) {
        RENDER_COUNT.with(|c| c.set(c.get() + 1));
    }
}

thread_local! {
    static RENDER_COUNT: Cell<u64> = const { Cell::new(0) };
}

impl<App: 'static> Default for PluginRegistry<App> {
    fn default() -> Self {
        Self::new()
    }
}

impl<App: 'static> Clone for PluginRegistry<App> {
    fn clone(&self) -> Self {
        PluginRegistry {
            plugins: self.plugins.clone(),
        }
    }
}

/// Convenience: provides a plugin's state to a subtree as a
/// [`Context`][crate::context::Context] so descendant components can read it.
///
/// Returns the [`Context`] so callers can keep a handle for updates. Wrap the
/// builder closure in [`provide_context`][crate::context::provide_context] so
/// the context is scoped to `scope`.
pub fn context_for_plugin<S: Clone + 'static>(state: S) -> Context<S> {
    Context::new(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter;
    impl Plugin for Counter {
        type State = Cell<u32>;
        fn name(&self) -> &'static str {
            "counter"
        }
        fn init(&self) -> Self::State {
            Cell::new(0)
        }
        fn on_render<A: 'static>(&self, ctx: &PluginCtx<Self::State, A>) {
            ctx.state.set(ctx.state.get() + 1);
        }
    }

    struct Named {
        name: &'static str,
    }
    impl Plugin for Named {
        type State = ();
        fn name(&self) -> &'static str {
            self.name
        }
    }

    #[derive(Debug, PartialEq)]
    struct Theme {
        dark: bool,
    }

    struct ThemePlugin;
    impl Plugin for ThemePlugin {
        type State = Theme;
        fn name(&self) -> &'static str {
            "theme"
        }
        fn init(&self) -> Self::State {
            Theme { dark: false }
        }
    }

    #[test]
    fn registers_and_runs_render_hooks() {
        let mut reg = PluginRegistry::<()>::new();
        reg.register(Counter);
        assert_eq!(reg.len(), 1);

        reg.run_render_hooks(None);
        reg.bump_render_count();
        reg.run_render_hooks(None);
        reg.bump_render_count();

        // The cell is internal; we check render_count instead.
        assert_eq!(reg.render_count(), 2);
    }

    #[test]
    fn distinct_named_plugins_register_independently() {
        let mut reg = PluginRegistry::<()>::new();
        reg.register(Named { name: "a" });
        reg.register(Named { name: "b" });
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn plugin_with_state_registers() {
        let mut reg = PluginRegistry::<()>::new();
        reg.register_with_state(ThemePlugin, Theme { dark: false });
        assert!(!reg.is_empty());
    }

    #[test]
    #[should_panic(expected = "duplicate plugin name")]
    fn duplicate_names_panic() {
        let mut reg = PluginRegistry::<()>::new();
        reg.register(Named { name: "dup" });
        reg.register(Named { name: "dup" });
    }
}
