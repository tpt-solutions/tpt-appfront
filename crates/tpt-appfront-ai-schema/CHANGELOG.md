# Changelog

All notable changes to `tpt-appfront-ai-schema` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `to_ai_schema_value` returns `Result<Value, serde_json::Error>` instead of
  panicking on malformed input.

### Fixed
- `serde_json::Map` insertions now use `insert` (was `map["key"] = val`, which
  panicked on missing keys).

## [0.1.0]

### Added
- Initial release: `to_json_ld` (schema.org `JSON-LD`), `to_ai_schema` /
  `to_ai_schema_value` (custom AI Schema with `InteractiveElement`/`DataElement`/
  `AiSchemaOutput`), and `both` returning both formats.
