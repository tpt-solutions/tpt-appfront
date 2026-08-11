# Changelog

All notable changes to `tpt-appfront-mcp` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `McpServer::run()` JSON-RPC 2.0 stdio loop (newline-delimited) with built-in
  `query_state`/`navigate` tools plus one generated tool per interactive node's
  `AiMeta`.

## [0.1.0]

### Added
- Initial release: MCP server exposing the Phase 10 agent API as JSON-RPC tools over
  stdio, wired to `tpt_appfront_core::query_state`/`navigate_to` and an
  app-supplied `on_command` closure (mirroring `tpt-appfront-server`'s `POST /command`).
