//! Desktop webview shell hosting the `ui/` `tpt-appfront-dom` counter app.
//!
//! Build/run:
//! ```text
//! cd ui && trunk build      # produces ui/dist/index.html (+ assets)
//! cargo run                 # opens the webview, hosting ui/dist
//! ```
//! Or use the CLI: `appfront dev --desktop-webview` (from this directory).
//!
//! Demonstrates the Phase 1 `AppBuilder` API: multi-window-ready, sidecar-
//! aware, with the built-in capability set (dialog/notify/clipboard/media/
//! secret) auto-granted through the ACL.

use anyhow::Result;
use tpt_appfront_webview::{run, WebviewOptions};
use std::path::PathBuf;

fn main() -> Result<()> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dist = PathBuf::from(manifest_dir).join("ui").join("dist");
    if !dist.join("index.html").exists() {
        eprintln!(
            "Missing {}/index.html.\nBuild the UI first:\n  cd ui && trunk build",
            dist.display()
        );
        std::process::exit(1);
    }

    // Keep the previous single-window `run()` contract available: the app still
    // defines a [`WebviewOptions`], but we route it through the new builder so
    // the built-in native capabilities (dialog/notify/clipboard/media/secret)
    // are ACL-gated and dispatched alongside the app's own `increment` action.
    let opts = WebviewOptions {
        title: "Counter — AppFront Webview".into(),
        width: 480,
        height: 360,
        dist_dir: dist.clone(),
        acl: appfront_webview::Acl {
            capabilities: vec![appfront_webview::Capability {
                action: "increment".into(),
                params: vec![],
            }],
        },
        max_commands_per_second: 20,
    };

    AppBuilder::new("tpt-counter-webview")
        .with_window(appfront_webview::WindowConfig::from_options("main", &opts))
        .with_acl(opts.acl.clone())
        .with_max_commands_per_second(opts.max_commands_per_second)
        .with_builtin_capabilities()
        .run(|action, params| {
            match action {
                "increment" => {
                    println!("[host] received `{action}` from webview (params: {params})");
                    Ok(())
                }
                // Built-in actions (dialog/notify/clipboard/media/secret) are
                // handled by the shell before reaching here; these synthetic
                // events come from the event loop.
                "shortcut" | "deeplink" | "filedrop" => {
                    println!("[host] event `{action}`: {params}");
                    Ok(())
                }
                other => Err(format!("unknown action `{other}`")),
            }
        })
}
