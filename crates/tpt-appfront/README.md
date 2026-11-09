# tpt-appfront

The umbrella crate for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront): one `UITree`
and signal core, with each backend behind a feature flag.

```toml
[dependencies]
tpt-appfront = { version = "0.1", features = ["html", "test"] }
```

```rust
use tpt_appfront::prelude::*;
```

Features: `dom`, `canvas`, `tui`, `html`, `ai-schema`, `server`, `mcp`, `test`. The
`webview` backend is not included because it is `publish = false`; see the crate docs.

The per-backend crates (`tpt-appfront-dom`, `tpt-appfront-canvas`, ...) remain the
primary entry points for backend-specific APIs.
