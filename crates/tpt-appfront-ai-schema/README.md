# tpt-appfront-ai-schema

JSON-LD (schema.org) and custom AI Schema output backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront), for AI-agent clients.

Renders a `UITree<Msg>` into JSON-LD rich snippets (`json_ld.rs`) and a custom AI-agent schema describing interactive elements, actions, and their parameters (`ai_schema.rs`). Format documented in [docs/ai-schema.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/ai-schema.md).

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-ai-schema = "0.1"
```

See the [workspace README](https://github.com/tpt-solutions/tpt-appfront#readme) and the [`ai-agent-demo` example](https://github.com/tpt-solutions/tpt-appfront/tree/main/examples/ai-agent-demo).

## License

Apache-2.0
