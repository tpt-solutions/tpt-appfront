//! Desktop webview shell hosting the `ui/` `appfront-dom` counter app.
//!
//! Build/run:
//! ```text
//! cd ui && trunk build      # produces ui/dist/index.html (+ assets)
//! cargo run                 # opens the webview, hosting ui/dist
//! ```
//! Or use the CLI: `appfront dev --desktop-webview` (from this directory).

use anyhow::Result;
use appfront_webview::{run, WebviewOptions};
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

    let opts = WebviewOptions {
        title: "Counter — AppFront Webview".into(),
        width: 480,
        height: 360,
        dist_dir: dist,
        // Only `increment` may be dispatched back to native from the page.
        allowed_actions: vec!["increment".into()],
        max_commands_per_second: 20,
    };

    run(opts, |action, params| {
        match action {
            "increment" => {
                println!("[host] received `{action}` from webview (params: {params})");
                Ok(())
            }
            other => Err(format!("unknown action `{other}`")),
        }
    })
}
