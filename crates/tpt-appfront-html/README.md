# tpt-appfront-html

Semantic HTML (SSR/SSG) backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront), with `data-ai-*` and OpenGraph tags for crawlers.

Renders a `UITree<Msg>` to a semantic HTML string, including `data-ai-action` attributes for AI-agent readability and OpenGraph tags for social-bot crawls, with inline styles from `tpt-appfront-core`'s styling utilities.

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-html = "0.1"
```

See the [workspace README](https://github.com/tpt-solutions/tpt-appfront#readme) and the [`ssr-page` example](https://github.com/tpt-solutions/tpt-appfront/tree/main/examples/ssr-page).

## License

Apache-2.0
