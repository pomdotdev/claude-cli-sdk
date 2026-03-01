# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-02-28

### Added

- `input_format` as a first-class `ClientConfig` field (`INPUT_FORMAT_STREAM_JSON` constant)
- `OUTPUT_FORMAT_STREAM_JSON` constant for the default NDJSON output format
- `init_stdin_message` config field to unblock the init handshake in `stream-json` input mode
- `Client::write_to_stdin()` for sending raw stdin writes outside of a `send()` turn
- Guard in `write_to_stdin()` that panics in debug builds if called during an active turn

### Fixed

- Removed `CI=true` default from spawned CLI environment — Claude Code CLI v2.1+ suppresses all output when `CI` is set, silently breaking NDJSON streaming
- `init_stdin_message` is now validated as JSON before the CLI is spawned
- Nested-session environment variables are stripped to prevent interference in `stream-json` mode

## [0.4.0] - 2026-02-22

### Added

- `query()` and `query_stream()` free functions for one-shot CLI interactions
- `query_with_content()` and `query_stream_with_content()` for multi-modal queries
- `Client` struct for stateful, multi-turn sessions with `send()` and `receive_messages()`
- `ClientConfig` with typed-builder pattern and configurable timeouts (`connect_timeout`, `turn_timeout`, `control_request_timeout`)
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
- Cooperative cancellation tokens
- Unix-only platform guard (macOS/Linux)
- 10 numbered examples with i18n documentation (English, Spanish, Japanese, Korean, Simplified Chinese)

[Unreleased]: https://github.com/pomdotdev/claude-cli-sdk/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/pomdotdev/claude-cli-sdk/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/pomdotdev/claude-cli-sdk/releases/tag/v0.4.0
