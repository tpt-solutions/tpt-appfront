# tpt-appfront-html

The semantic HTML (SSR/SSG) backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Renders a `UITree<Msg>` to a semantic HTML5 string — used for server-side
rendering, static site generation, and crawler/SEO responses. Because the same
`UITree` drives every backend, your server-rendered HTML and your client-rendered
DOM are guaranteed to match.

## Features

- **`render`** — a semantic HTML fragment (no `<html>`/`<head>`/`<body>`).
- **`render_page`** — a full HTML5 page with OpenGraph meta tags (for social bots).
- **`data-ai-action` / `data-ai-params`** attributes — emitted for interactive
  nodes so AI crawlers/agents can discover actions.
- **Pure and backend-agnostic** — `UITree`/`NodeMeta` carry no DOM assumptions,
  so SSR ignores `VirtualScroll` (renders the full list, which crawlers need) while
  the DOM/canvas backends window it.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-html = "0.1"
```

## Example

```rust
use tpt_appfront_core::UITree;

let html = tpt_appfront_html::render_page(&ui, "My App", "A TPT AppFront app");
println!("{html}");
```

Usually you'll serve this from `tpt-appfront-server`'s smart router, which picks
HTML automatically for crawler/social clients. See
[docs/quickstart.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/quickstart.md).

## License

MIT OR Apache-2.0
