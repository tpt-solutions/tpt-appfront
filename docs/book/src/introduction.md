# TPT AppFront

You describe a UI once, as a `UITree<Msg>` built in Rust. Each backend turns that tree into its own output:

| Backend | Crate | Output |
|---|---|---|
| DOM | `tpt-appfront-dom` | Real DOM in the browser (WebAssembly) |
| Canvas | `tpt-appfront-canvas` | A native window, or `<canvas>` on the web |
| TUI | `tpt-appfront-tui` | A terminal UI (ratatui) |
| Webview | `tpt-appfront-webview` | An OS webview window hosting the DOM build |
| HTML | `tpt-appfront-html` | Semantic HTML for crawlers (SSR/SSG) |
| AI schema | `tpt-appfront-ai-schema` | JSON-LD and an agent-readable schema |

The message type `Msg` is yours. The core knows nothing about your events.

This guide covers the parts you use day to day. The API reference is in the crate docs. Support per
backend is listed in `docs/backend-capabilities.md` in the repository.

Snippets are sketches. They show the shape of the API, and the book build does not compile them.
