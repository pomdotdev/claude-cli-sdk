# claude-cli-sdk

**English** | [简体中文](README_zh-CN.md) | [日本語](README_ja.md) | [한국어](README_ko.md) | [Español](README_es.md)

[![Crates.io](https://img.shields.io/crates/v/claude-cli-sdk.svg)](https://crates.io/crates/claude-cli-sdk)
[![docs.rs](https://docs.rs/claude-cli-sdk/badge.svg)](https://docs.rs/claude-cli-sdk)
[![CI](https://github.com/pomdotdev/claude-cli-sdk/actions/workflows/ci.yml/badge.svg)](https://github.com/pomdotdev/claude-cli-sdk/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)

Strongly-typed, async-first Rust SDK for building agents on the [Claude Code CLI](https://www.anthropic.com/claude-code).

## Features

- **One-shot queries** — `query()` sends a prompt and collects all response messages
- **Streaming** — `query_stream()` yields messages as they arrive
- **Multi-turn sessions** — `Client` with `send()` / `receive_messages()` for persistent conversations
- **Multimodal input** — send images (URL or base64) alongside text via `query_with_content()`
- **Permission callbacks** — `CanUseToolCallback` for programmatic per-tool approval/denial
- **8 lifecycle hooks** — `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `UserPromptSubmit`, `Stop`, `SubagentStop`, `PreCompact`, `Notification`
- **Extended thinking** — `max_thinking_tokens` to enable chain-of-thought reasoning
- **Fallback model** — `fallback_model` for automatic model failover
- **Dynamic control** — `set_model()` and `set_permission_mode()` mid-session
- **Cooperative cancellation** — `CancellationToken` for graceful early termination
- **Message callback** — observe, transform, or filter messages before they reach your code
- **Stderr callback** — capture CLI debug output for logging/diagnostics
- **MCP server config** — attach external Model Context Protocol servers
- **Testing framework** — `MockTransport`, `ScenarioBuilder`, and message builders for unit tests without a live CLI
- **Cross-platform** — macOS, Linux, and Windows

## Quick Start

```toml
[dependencies]
claude-cli-sdk = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
use claude_cli_sdk::{query, ClientConfig};

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let config = ClientConfig::builder()
        .prompt("What is Rust?")
        .build();

    let messages = query(config).await?;

    for msg in &messages {
        if let Some(text) = msg.assistant_text() {
            println!("{text}");
        }
    }
    Ok(())
}
```

## Architecture

```
┌──────────┐    ┌──────────────┐    ┌───────────┐    ┌─────────────┐    ┌───────────┐
│ Your Code │───▶│ ClientConfig │───▶│  Client   │───▶│  Transport  │───▶│ Claude    │
│           │    │              │    │           │    │ (CliTransport│    │ Code CLI  │
│ query()   │    │ model        │    │ connect() │    │  or Mock)   │    │           │
│ query_    │    │ hooks        │    │ send()    │    │             │    │ NDJSON    │
│  stream() │    │ permissions  │    │ close()   │    │ stdin/stdout│───▶│ stdio     │
│ Client    │    │ callbacks    │    │ set_model()│   │ NDJSON      │    │ protocol  │
└──────────┘    └──────────────┘    └───────────┘    └─────────────┘    └───────────┘
                                          │                                    │
                                          ▼                                    ▼
                                    Background task               Claude API (Anthropic)
                                    routes: hooks,
                                    permissions,
                                    callbacks
```

## Examples

All examples are runnable with `cargo run --example <name>`. They require a working Claude Code CLI installation.

| Example | Feature | Run |
|---------|---------|-----|
| [`01_basic_query`](examples/01_basic_query.rs) | One-shot query | `cargo run --example 01_basic_query` |
| [`02_streaming`](examples/02_streaming.rs) | Streaming + model selection | `cargo run --example 02_streaming` |
| [`03_multi_turn`](examples/03_multi_turn.rs) | Multi-turn sessions | `cargo run --example 03_multi_turn` |
| [`04_permissions`](examples/04_permissions.rs) | Permission callbacks | `cargo run --example 04_permissions` |
| [`05_hooks`](examples/05_hooks.rs) | Lifecycle hooks (3 events) | `cargo run --example 05_hooks` |
| [`06_multimodal`](examples/06_multimodal.rs) | Image + text input | `cargo run --example 06_multimodal` |
| [`07_thinking_and_fallback`](examples/07_thinking_and_fallback.rs) | Extended thinking + fallback model | `cargo run --example 07_thinking_and_fallback` |
| [`08_cancellation`](examples/08_cancellation.rs) | Cooperative cancellation | `cargo run --example 08_cancellation` |
| [`09_message_callback`](examples/09_message_callback.rs) | Message observation + stderr debug | `cargo run --example 09_message_callback` |
| [`10_dynamic_control`](examples/10_dynamic_control.rs) | Runtime model switch + MCP servers | `cargo run --example 10_dynamic_control` |

## Core API

### One-shot query

```rust
use claude_cli_sdk::{query, ClientConfig};

let config = ClientConfig::builder()
    .prompt("List the files in /tmp")
    .build();

let messages = query(config).await?;
```

### Streaming

```rust
use claude_cli_sdk::{query_stream, ClientConfig};
use tokio_stream::StreamExt;

let config = ClientConfig::builder()
    .prompt("Explain ownership in Rust")
    .model("claude-opus-4-6")
    .build();

let stream = query_stream(config).await?;
tokio::pin!(stream);

while let Some(msg) = stream.next().await {
    let msg = msg?;
    if let Some(text) = msg.assistant_text() {
        print!("{text}");
    }
}
```

### Multi-turn sessions

```rust
use claude_cli_sdk::{Client, ClientConfig, Message};
use tokio_stream::StreamExt;

let config = ClientConfig::builder()
    .prompt("Start a Rust project")
    .build();

let mut client = Client::new(config)?;
let session_info = client.connect().await?;
println!("Session: {}", session_info.session_id);

// First turn (prompt sent via config).
{
    let stream = client.receive_messages()?;
    tokio::pin!(stream);
    while let Some(msg) = stream.next().await {
        let msg = msg?;
        if let Some(text) = msg.assistant_text() {
            print!("{text}");
        }
        if matches!(msg, Message::Result(_)) {
            break;
        }
    }
}

// Subsequent turns.
{
    let stream = client.send("Now add a test suite")?;
    tokio::pin!(stream);
    while let Some(msg) = stream.next().await {
        if let Some(text) = msg?.assistant_text() {
            print!("{text}");
        }
    }
}

client.close().await?;
```

## Advanced Features

### Permission callbacks

```rust
use claude_cli_sdk::{ClientConfig, PermissionDecision, PermissionContext};
use std::sync::Arc;

let config = ClientConfig::builder()
    .prompt("Edit main.rs")
    .can_use_tool(Arc::new(|tool_name: &str, _input: &serde_json::Value, _ctx: PermissionContext| {
        let tool_name = tool_name.to_owned();
        Box::pin(async move {
            if tool_name.starts_with("Read") || tool_name.starts_with("Write") {
                PermissionDecision::allow()
            } else {
                PermissionDecision::deny("Only file tools are allowed")
            }
        })
    }))
    .build();
```

### Lifecycle hooks

Register callbacks for 8 lifecycle events:

| Event | When it fires |
|-------|--------------|
| `PreToolUse` | Before a tool executes — can allow, block, modify input, or abort |
| `PostToolUse` | After a tool completes successfully |
| `PostToolUseFailure` | After a tool fails with an error |
| `UserPromptSubmit` | When a user prompt is submitted |
| `Stop` | When the agent session stops |
| `SubagentStop` | When a subagent session stops |
| `PreCompact` | Before context compaction occurs |
| `Notification` | General notification event |

```rust
use claude_cli_sdk::{ClientConfig, HookMatcher, HookEvent, HookOutput};
use std::sync::Arc;

let config = ClientConfig::builder()
    .prompt("Refactor auth module")
    .hooks(vec![
        HookMatcher::new(HookEvent::PreToolUse, Arc::new(|input, _session, _ctx| {
            Box::pin(async move {
                eprintln!("Tool: {:?}", input.tool_name);
                HookOutput::allow()
            })
        })).for_tool("Bash"),
    ])
    .build();
```

### Multimodal (images)

```rust
use claude_cli_sdk::{query_with_content, ClientConfig, UserContent};

let content = vec![
    UserContent::image_url("https://example.com/diagram.png", "image/png")?,
    UserContent::text("Describe this diagram"),
];
let messages = query_with_content(content, config).await?;
```

Also supports base64-encoded images via `UserContent::image_base64()`. Accepted MIME types: `image/jpeg`, `image/png`, `image/gif`, `image/webp`. Maximum base64 payload: 15 MiB.

### Extended thinking

```rust
let config = ClientConfig::builder()
    .prompt("Solve this step by step")
    .max_thinking_tokens(10_000_u32)
    .build();
```

Thinking blocks appear as `ContentBlock::Thinking` in the response:

```rust
use claude_cli_sdk::ContentBlock;

for block in &assistant.message.content {
    if let ContentBlock::Thinking(t) = block {
        println!("Thinking: {}", t.thinking);
    }
}
```

### Fallback model

```rust
let config = ClientConfig::builder()
    .prompt("Complex task")
    .model("claude-sonnet-4-5")
    .fallback_model("claude-haiku-4-5")
    .build();
```

### Dynamic control

Change model or permission mode mid-session:

```rust
// Switch model during a conversation
client.set_model(Some("claude-haiku-4-5")).await?;

// Revert to session default
client.set_model(None).await?;

// Change permission mode
client.set_permission_mode(PermissionMode::AcceptEdits).await?;

// Send interrupt signal (SIGINT)
client.interrupt().await?;
```

### Message callback

Observe, transform, or filter messages before they reach your code:

```rust
use claude_cli_sdk::{ClientConfig, Message, MessageCallback};
use std::sync::Arc;

let callback: MessageCallback = Arc::new(|msg: Message| {
    eprintln!("received: {msg:?}");
    Some(msg) // pass through (return None to filter out)
});

let config = ClientConfig::builder()
    .prompt("Hello")
    .message_callback(callback)
    .build();
```

### Cancellation

```rust
use claude_cli_sdk::{query_stream, CancellationToken, ClientConfig};

let token = CancellationToken::new();
let config = ClientConfig::builder()
    .prompt("Long task")
    .cancellation_token(token.clone())
    .build();

// Cancel from another task:
token.cancel();

// Stream will yield Error::Cancelled, checkable via:
if error.is_cancelled() { /* handle gracefully */ }
```

### MCP servers

```rust
use claude_cli_sdk::{ClientConfig, McpServerConfig, McpServers};

let mut servers = McpServers::new();
servers.insert(
    "filesystem".into(),
    McpServerConfig::new("npx")
        .with_args(["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]),
);

let config = ClientConfig::builder()
    .prompt("List files using the filesystem MCP server")
    .mcp_servers(servers)
    .build();
```

### Stderr debugging

Capture the CLI's stderr output for diagnostics:

```rust
use std::sync::Arc;

let config = ClientConfig::builder()
    .prompt("Debug this")
    .verbose(true)
    .stderr_callback(Arc::new(|line: String| {
        eprintln!("[stderr] {line}");
    }))
    .build();
```

## `ClientConfig` Reference

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `prompt` | `String` | **Required** | Initial prompt text |
| `model` | `Option<String>` | `None` | Model name, e.g. `"claude-sonnet-4-5"` |
| `fallback_model` | `Option<String>` | `None` | Fallback if primary model is unavailable |
| `cwd` | `Option<PathBuf>` | `None` | Working directory for the CLI process |
| `max_turns` | `Option<u32>` | `None` | Maximum agentic turns before stopping |
| `max_budget_usd` | `Option<f64>` | `None` | Cost cap for the session |
| `max_thinking_tokens` | `Option<u32>` | `None` | Maximum extended thinking tokens |
| `permission_mode` | `PermissionMode` | `Default` | `Default`, `AcceptEdits`, `Plan`, or `BypassPermissions` |
| `can_use_tool` | `Option<CanUseToolCallback>` | `None` | Per-tool permission callback |
| `system_prompt` | `Option<SystemPrompt>` | `None` | Text or preset system prompt |
| `allowed_tools` | `Vec<String>` | `[]` | Allowlist of tool names |
| `disallowed_tools` | `Vec<String>` | `[]` | Blocklist of tool names |
| `mcp_servers` | `McpServers` | `{}` | External MCP server definitions |
| `hooks` | `Vec<HookMatcher>` | `[]` | Lifecycle hook registrations |
| `message_callback` | `Option<MessageCallback>` | `None` | Message observe/filter callback |
| `resume` | `Option<String>` | `None` | Resume an existing session by ID |
| `verbose` | `bool` | `false` | Enable verbose CLI output |
| `cancellation_token` | `Option<CancellationToken>` | `None` | Cooperative cancellation token |
| `stderr_callback` | `Option<Arc<dyn Fn(String)>>` | `None` | Stderr output callback |
| `connect_timeout` | `Option<Duration>` | `30s` | Deadline for spawn + init |
| `close_timeout` | `Option<Duration>` | `10s` | Deadline for graceful shutdown |
| `read_timeout` | `Option<Duration>` | `None` | Per-message recv deadline |
| `default_hook_timeout` | `Duration` | `30s` | Hook callback fallback timeout |
| `version_check_timeout` | `Option<Duration>` | `5s` | `--version` check deadline |

Set any `Option<Duration>` timeout to `None` to wait indefinitely.

## Python SDK Comparison

| Capability | Python SDK (`claude-code-sdk`) | This crate (`claude-cli-sdk`) |
|-----------|-------------------------------|------------------------------|
| One-shot query | `query()` | `query()` |
| Streaming | `query_stream()` | `query_stream()` |
| Multi-turn sessions | `ClaudeCodeSession` | `Client` |
| Permission callbacks | `can_use_tool` | `CanUseToolCallback` |
| Lifecycle hooks | `hooks` | `HookMatcher` (8 events) |
| MCP servers | `mcp_servers` | `McpServers` |
| Multimodal (images) | `Content` blocks | `UserContent` + `query_with_content()` |
| Extended thinking | `max_thinking_tokens` | `max_thinking_tokens` |
| Fallback model | `fallback_model` | `fallback_model` |
| Dynamic model switch | — | `Client::set_model()` |
| Dynamic permission mode | — | `Client::set_permission_mode()` |
| Cooperative cancellation | — | `CancellationToken` |
| Message callback | — | `MessageCallback` |
| Stderr callback | — | `stderr_callback` |
| Testing framework | — | `MockTransport` + `ScenarioBuilder` |
| Type safety | Runtime | Compile-time (typed builder) |

## Testing

Enable the `testing` feature for unit tests without a live CLI:

```toml
[dev-dependencies]
claude-cli-sdk = { version = "0.1", features = ["testing"] }
```

```rust
use std::sync::Arc;
use claude_cli_sdk::Client;
use claude_cli_sdk::config::ClientConfig;
use claude_cli_sdk::testing::{ScenarioBuilder, assistant_text};

let transport = ScenarioBuilder::new("test-session")
    .exchange(vec![assistant_text("Hello!")])
    .build();
let transport = Arc::new(transport);

let mut client = Client::with_transport(
    ClientConfig::builder().prompt("test").build(),
    transport,
).unwrap();
```

`ScenarioBuilder` queues init + exchange messages so your tests exercise real `Client` logic without spawning a CLI process.

## Troubleshooting

| Problem | Cause | Fix |
|---------|-------|-----|
| `CliNotFound` error | Claude Code CLI not on `PATH` | Install: `npm install -g @anthropic-ai/claude-code` |
| Timeout on `connect()` | CLI slow to start or not responding | Increase `connect_timeout` or check CLI installation |
| Session hangs on permission request | `can_use_tool` callback not set but CLI requests permission | Set `can_use_tool` or use `PermissionMode::BypassPermissions` for CI |
| "Client dropped without calling close()" warning | `Client` dropped before `close()` | Call `client.close().await` before dropping, or use scoped blocks |
| Noisy stderr output | CLI prints debug info to stderr | Set `stderr_callback` to capture/filter, or omit `verbose(true)` |
| `VersionMismatch` error | CLI version below SDK minimum | Update CLI: `npm update -g @anthropic-ai/claude-code` |

## Feature Flags

| Feature | Description |
|---------|-------------|
| `testing` | `MockTransport`, `ScenarioBuilder`, and message builder helpers for unit tests |
| `efficiency` | Reserved for future throughput optimizations |
| `integration` | Integration test helpers (requires a live CLI) |

## Platform Support

macOS, Linux, and Windows.

## Disclaimer

This is an unofficial SDK, community-developed by the [POM](https://pom.dev) team, and is not affiliated with, endorsed by, or sponsored by Anthropic, PBC. "Claude" and "Claude Code" are trademarks of Anthropic. This crate interacts with the Claude Code CLI but does not contain any Anthropic proprietary code.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
