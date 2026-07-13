# appfront-ai-schema

JSON-LD and AI-agent schema backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Renders an `appfront-core::UITree<Msg>` into two machine-readable formats: `json_ld.rs` produces schema.org JSON-LD (rich snippets for search engines), and `ai_schema.rs` produces a custom AI-agent schema describing interactive elements, their actions, and parameters — designed to let LLM-based agents understand and drive a UI without a browser.

Typically served behind `appfront-server`'s smart router to clients classified as AI agents. The AI-schema format is frozen and documented in [docs/ai-schema.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/ai-schema.md).

See the [project README](https://github.com/tpt-solutions/tpt-appfront#readme) for the full architecture, and `examples/ai-agent-demo` for a headless demo of the `query_state`/`navigate_to`/`trigger_event` API.
