# tpt-appfront-ai-schema

The AI Schema backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Turns a `UITree<Msg>` into machine-readable output an LLM or agent can consume:
[JSON-LD](https://json-ld.org/) (schema.org structured data) and a custom
**AI Schema** that lists interactive elements, their actions, and their
parameters, plus the data elements on the page. The exact shapes are described in
[docs/ai-schema.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/ai-schema.md).

## API

- **`to_json_ld(ui: &UITree<Msg>) -> serde_json::Value`**: a JSON-LD object with
  `@context` and an `@graph` array. Embed it in `<script type="application/ld+json">`.
- **`to_ai_schema(ui: &UITree<Msg>) -> AiSchemaOutput`**: the custom AI Schema, with
  `schema_version`, `title`, `interactive`, and `data`. The element types are
  `InteractiveElement` and `DataElement`.
- **`to_ai_schema_value(ui: &UITree<Msg>) -> Result<serde_json::Value, serde_json::Error>`**:
  the same schema as a JSON value. It returns an error rather than panicking.
- **`both(ui: &UITree<Msg>) -> (serde_json::Value, AiSchemaOutput)`**: JSON-LD and
  the AI Schema in one call.

The output renders the full tree and ignores `VirtualScroll`, because an agent
needs the complete element set.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-ai-schema = "0.1"
serde_json = "1"   # to print the output
```

This crate has no Cargo features.

## Example

```rust
use tpt_appfront_core::UITree;

fn describe(ui: &UITree<()>) -> serde_json::Result<()> {
    let (json_ld, ai_schema) = tpt_appfront_ai_schema::both(ui);
    println!("{}", serde_json::to_string_pretty(&json_ld)?);
    println!("{}", serde_json::to_string_pretty(&ai_schema)?);
    Ok(())
}
```

Typically served by `tpt-appfront-server`'s smart router, which selects this
backend for AI-agent `User-Agent`s.

## License

MIT OR Apache-2.0
