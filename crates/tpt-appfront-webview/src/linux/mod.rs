//! Linux backend. Stage 1 stub: the GTK3 / WebKitGTK backend is built in stages 2-4
//! of `docs/linux-webview-backend.md`. Until then, running an app on Linux returns a
//! clear error instead of building `tao`/`wry`, which is what pulls in `target-lexicon`.

// Stage 2 builds the loader and symbol table; stage 3 is the first caller.
#[allow(dead_code)]
mod loader;
#[allow(dead_code)]
mod sys;

use std::rc::Rc;

use anyhow::{bail, Result};

use crate::app::{AppBuilder, CommandHandler};

impl AppBuilder {
    /// Runs the Linux event loop. Not implemented yet.
    pub(crate) fn run_native(self, _on_command: Rc<CommandHandler>) -> Result<()> {
        // Dropping the builder here also drops anything the startup prefix took ownership of.
        drop(self);
        bail!(
            "the Linux webview backend is not implemented yet; see docs/linux-webview-backend.md \
             (stage 3). Build and run this app on Windows or macOS for now"
        )
    }
}
