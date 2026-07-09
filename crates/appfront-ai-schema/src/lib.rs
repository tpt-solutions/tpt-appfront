//! AI Schema backends: `UITree` → JSON-LD (structured data) and
//! `UITree` → custom AI Schema (interactive elements, actions, params).
//! See `docs/ai-schema.md` for the format definitions.

mod ai_schema;
mod json_ld;

pub use ai_schema::{to_ai_schema, to_ai_schema_value, AiSchemaOutput, DataElement, InteractiveElement};
pub use json_ld::to_json_ld;

use appfront_core::UITree;

/// Convenience: returns both formats as a pair `(json_ld, ai_schema)`.
pub fn both<Msg>(ui: &UITree<Msg>) -> (serde_json::Value, ai_schema::AiSchemaOutput) {
    (json_ld::to_json_ld(ui), ai_schema::to_ai_schema(ui))
}
