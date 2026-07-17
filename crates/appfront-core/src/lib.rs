pub mod agent;
pub mod component;
pub mod components;
pub mod context;
pub mod devtools;
pub mod error_boundary;
pub mod form;
pub mod plugin;
pub mod reconcile;
pub mod resource;
pub mod router;
pub mod signal;
pub mod static_tree;
pub mod store;
pub mod styling;
pub mod suspense;
pub mod ui_tree;
pub mod virtual_scroll;

pub use context::{provide_context, use_context, Context};
pub use resource::{Resource, ResourceState};
pub use store::{instrument_store, Persistence, Store, StoreSubscription};
pub use suspense::Suspense;
pub use router::{Route, RouteTable, Router};

/// Tailwind-style utility-class macro.
///
/// ```ignore
/// ui.class(class!("bg-blue-500", "p-4", "rounded-lg"));
/// ```
///
/// Emits a `String` of space-separated, `af-u-`-prefixed utility names and
/// **validates each name at compile time** against
/// [`styling::UTILITIES`] — an unknown utility is a build error, not
/// silently-unstyled output. The `af-u-` prefix matches the rules produced
/// by [`styling::style_sheet`] (embed once in a page `<head>`), or use
/// [`styling::inline_style`] / the `appfront-html` SSR backend to have the
/// CSS applied directly.
#[macro_export]
macro_rules! class {
    ($($u:literal),* $(,)?) => {{
        // Compile-time validation: each literal must be a known utility.
        $( $crate::styling::class_macro_check($u); )*
        let mut __appfront_class = ::std::string::String::new();
        $(
            __appfront_class.push_str("af-u-");
            __appfront_class.push_str($u);
            __appfront_class.push(' ');
        )*
        __appfront_class.truncate(__appfront_class.trim_end().len());
        __appfront_class
    }};
}

pub use agent::{current_route, navigate_to, query_state, route_signal, trigger_event, AgentState, ElementSummary};
pub use appfront_macros::component;
pub use appfront_macros::rsx;
pub use appfront_macros::view;
pub use devtools::{inspect_state, inspect_tree, render, to_html, DevtoolsReport};
pub use error_boundary::{error_boundary, recover_or, BoundaryResult};
pub use form::FormState;
pub use plugin::{context_for_plugin, Plugin, PluginCtx, PluginRegistry};
pub use reconcile::{
    apply_edits, diff_summary, edit_description, reconcile_keys, History, KeyedDiff, ListEdit,
};
pub use signal::{
    batch, create_effect, create_memo, reset_signal_activity, set_hydration_state, signal_activity,
    take_hydration_state, EffectHandle, Signal,
};
pub use static_tree::static_node;
pub use styling::{
    class_macro_check, class_value, inline_style, is_utility, lookup, style_sheet, UTILITIES,
};
pub use ui_tree::{AiMeta, ContainerBuilder, HydrationPayload, NodeKind, NodeMeta, NodeRef, UITree};
pub use virtual_scroll::{VirtualScroll, VisibleRange};
pub use component::{memoize, Children};
pub use components::{
    announce_resource_status, date_picker, dropdown, live_region, modal, move_focus,
    sortable_table, tabs,
};
