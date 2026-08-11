# tpt-appfront-ai-schema

The AI Schema backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Turns a `UITree<Msg>` into machine-readable representations an LLM or agent can
consume: [JSON-LD](https://json-ld.org/) (schema.org structured data / rich
snippets) and a custom **AI Schema** describing interactive elements, their
actions, and parameters. See [docs/ai-schema.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/ai-schema.md)
for the exact shapes.

## Features

- **`to_json_ld`** — emits schema.org `JSON-LD` for the tree.
- **`to_ai_schema`** / **`to_ai_schema_value`** — emits the custom AI Schema
  (`InteractiveElement`/`DataElement`/`AiSchemaOutput`). `to_ai_schema_value`
  returns `Result<Value, serde_json::Error>` (never panics).
- **`both`** — returns `(json_ld, ai_schema)` in one call.
- **SSR-style completeness** — ignores `VirtualScroll` and renders the full tree,
  since an agent needs the complete element set.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-ai-schema = "0.1"
```

## Example

```rust
use tpt_appfront_core::UITree;

let (json_ld, ai_schema) = tpt_appfront_ai_schema::both(&ui);
println!("{}", serde_json::to_string_pretty(&json_ld).unwrap());
println!("{}", serde_json::to_string_pretty(&ai_schema).unwrap());
```

Typically served by `tpt-appfront-server`'s smart router, which selects this
backend for AI-agent `User-Agent`s.

## License

MIT OR Apache-2.0
