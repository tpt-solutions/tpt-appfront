# tpt-appfront-html

The semantic HTML (SSR/SSG) backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Renders a `UITree<Msg>` to a semantic HTML5 string, for server-side rendering,
static site generation, and crawler/SEO responses. Because the same `UITree`
drives every backend, the server-rendered HTML and the client-rendered DOM come
from the same tree.

## API

- **`render(ui: &UITree<Msg>) -> String`**: an HTML fragment, with no
  `<html>`, `<head>`, or `<body>`.
- **`render_page(ui: &UITree<Msg>, title: &str, description: &str) -> String`**:
  a full HTML5 document with OpenGraph `og:title` and `og:description` tags.
- **`esc_attr(s: &str) -> String`** and **`esc_script_json(json: &str) -> String`**:
  the escaping helpers the renderer uses, exposed for callers that build markup
  around the output.

Output details:

- Interactive nodes get `data-ai-action` and `data-ai-params` attributes, so AI
  crawlers and agents can find actions.
- Utility classes from `tpt_appfront_core::styling` are emitted as inline `style`
  attributes.
- Accessibility attributes (ARIA roles, `aria-label`, `<label for>`) are emitted
  for form controls and grids.
- `VirtualScroll` is ignored. The full list is rendered, because crawlers need the
  complete content. The DOM and canvas backends do the windowing.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-html = "0.1"
```

This crate has no Cargo features.

## Example

```rust
use tpt_appfront_core::UITree;

fn home() -> String {
    let ui: UITree<()> = UITree::container(|c| {
        c.heading(1, "Welcome");
        c.text("Server-rendered content for crawlers.");
    });
    tpt_appfront_html::render_page(&ui, "My App", "A TPT AppFront app")
}
```

In production, `tpt-appfront-server`'s smart router usually picks this backend
for crawler and social-bot requests. See
[docs/quickstart.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/quickstart.md).

## License

MIT OR Apache-2.0
