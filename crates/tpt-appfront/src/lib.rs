//! TPT AppFront umbrella crate.
//!
//! One `UITree` and signal core, with each backend behind a feature flag:
//!
//! | feature     | module                  | what it gives you                          |
//! |-------------|-------------------------|--------------------------------------------|
//! | `dom`       | [`dom`]                 | reactive DOM (wasm32 only)                 |
//! | `canvas`    | [`canvas`]              | taffy canvas with an internal window layer (native) |
//! | `tui`       | [`tui`]                 | ratatui terminal UI                        |
//! | `html`      | [`html`]                | semantic HTML for SSR/SSG                  |
//! | `ai-schema` | [`ai_schema`]           | JSON-LD and AI-agent schema                |
//! | `server`    | [`server`]              | Axum smart router                          |
//! | `mcp`       | [`mcp`]                 | MCP server over stdio                      |
//! | `test`      | [`test`]                | headless test harness and linter           |
//! | `templates` | [`templates`]           | prebuilt login, dashboard and settings UI  |
//!
//! The `webview` backend is not included: it is `publish = false`, and a published crate
//! cannot depend on an unpublished one. Use `tpt-appfront-webview` directly, from a
//! workspace or path dependency.
//!
//! The [`prelude`] brings in the names most apps need.
//!
//! `view!`, `rsx!` and `#[component]` resolve their core paths through the calling crate, so
//! depending on this umbrella alone is enough.

pub use tpt_appfront_core as core;

/// Re-exports of the core types and macros most apps use.
pub mod prelude {
    pub use tpt_appfront_core::{
        class, component, view, ContainerBuilder, FormState, Router, Signal, Store, UITree,
    };
}

#[cfg(feature = "ai-schema")]
pub use tpt_appfront_ai_schema as ai_schema;
#[cfg(feature = "canvas")]
pub use tpt_appfront_canvas as canvas;
#[cfg(feature = "dom")]
pub use tpt_appfront_dom as dom;
#[cfg(feature = "html")]
pub use tpt_appfront_html as html;
#[cfg(feature = "mcp")]
pub use tpt_appfront_mcp as mcp;
#[cfg(feature = "server")]
pub use tpt_appfront_server as server;
#[cfg(feature = "templates")]
pub use tpt_appfront_templates as templates;
#[cfg(feature = "test")]
pub use tpt_appfront_test as test;
#[cfg(feature = "tui")]
pub use tpt_appfront_tui as tui;

#[cfg(test)]
mod tests {
    use super::prelude::*;

    #[test]
    fn prelude_builds_a_tree_with_view_and_class() {
        #[derive(Debug, Clone, PartialEq)]
        enum Msg {
            Clicked,
        }

        let ui: UITree<Msg> = view! {
            <Container class="p-4">
                <Button on_click={ Msg::Clicked }>"Go"</Button>
            </Container>
        };
        assert_eq!(ui.meta.class.as_deref(), Some("p-4"));
    }

    #[test]
    fn class_macro_is_in_the_prelude() {
        let classes = class!("p-4", "bg-white");
        assert_eq!(classes, "af-u-p-4 af-u-bg-white");
    }
}
