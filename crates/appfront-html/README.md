# appfront-html

Semantic HTML/SSR backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Renders an `appfront-core::UITree<Msg>` to a semantic HTML string, suitable for server-side rendering, static-site generation, or serving to crawlers/social-media bots. Output includes `data-ai-action` attributes for interactive elements, OpenGraph tags for social-bot crawls, and inline `style` attributes derived from `appfront-core`'s styling system.

Typically used behind `appfront-server`'s smart router (which serves this backend to crawlers and social bots while humans get the WASM shell), or standalone for static-site generation.

See the [project README](https://github.com/tpt-solutions/tpt-appfront#readme) for the full architecture, and `examples/ssr-page` for a runnable example that prints a semantic HTML string.
