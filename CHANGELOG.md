# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-02-21

### Added

- `query()` and `query_stream()` free functions for one-shot CLI interactions
- `query_with_content()` and `query_stream_with_content()` for multi-modal queries
- `Client` struct for stateful, multi-turn sessions with `send()` and `receive_messages()`
- `ClientConfig` with typed-builder pattern and configurable timeouts
- `CliTransport` for spawning and communicating with the Claude Code CLI via NDJSON stdio
- `MockTransport` and `ScenarioBuilder` behind the `testing` feature flag
- Permission callbacks (`CanUseToolCallback`) for programmatic tool approval
- Lifecycle hooks (`HookMatcher`) for `PreToolUse`, `PostToolUse`, `Stop`, and other events
- Hook timeout enforcement with configurable default and per-hook overrides (fail-open)
- MCP server configuration support (`McpServers`, `SdkMcpServerConfig`)
- Message callback for observing or filtering messages before delivery
- CLI discovery via `find_cli()` with priority-ordered path search
- Version checking via `check_cli_version()` with configurable timeout
- Dynamic mid-session control: `set_model()`, `set_permission_mode()`
- Multi-modal content support (text, images via base64 and URL)
- Comprehensive error types with `is_retriable()` helper
- Unix-only platform guard (macOS/Linux)

[Unreleased]: https://github.com/pomdotdev/claude-cli-sdk/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/pomdotdev/claude-cli-sdk/releases/tag/v0.1.0
