# claude-cli-sdk

[English](README.md) | **简体中文** | [日本語](README_ja.md) | [한국어](README_ko.md) | [Español](README_es.md)

[![Crates.io](https://img.shields.io/crates/v/claude-cli-sdk.svg)](https://crates.io/crates/claude-cli-sdk)
[![docs.rs](https://docs.rs/claude-cli-sdk/badge.svg)](https://docs.rs/claude-cli-sdk)
[![CI](https://github.com/pomdotdev/claude-cli-sdk/actions/workflows/ci.yml/badge.svg)](https://github.com/pomdotdev/claude-cli-sdk/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#许可证)

强类型、异步优先的 Rust SDK，基于 [Claude Code CLI](https://www.anthropic.com/claude-code) 构建智能体。

## 功能特性

- **单次查询** — `query()` 发送提示并收集所有响应消息
- **流式传输** — `query_stream()` 实时输出消息
- **多轮会话** — `Client` 通过 `send()` / `receive_messages()` 支持持久对话
- **多模态输入** — 通过 `query_with_content()` 发送图片（URL 或 base64）和文本
- **权限回调** — `CanUseToolCallback` 实现逐工具的程序化审批/拒绝
- **8 个生命周期钩子** — `PreToolUse`、`PostToolUse`、`PostToolUseFailure`、`UserPromptSubmit`、`Stop`、`SubagentStop`、`PreCompact`、`Notification`
- **扩展思维** — `max_thinking_tokens` 启用思维链推理
- **备用模型** — `fallback_model` 自动模型故障转移
- **动态控制** — 会话中途调用 `set_model()` 和 `set_permission_mode()`
- **协作式取消** — `CancellationToken` 实现优雅的提前终止
- **消息回调** — 在消息到达代码前进行观察、转换或过滤
- **标准错误回调** — 捕获 CLI 调试输出用于日志/诊断
- **MCP 服务器配置** — 附加外部 Model Context Protocol 服务器
- **测试框架** — `MockTransport`、`ScenarioBuilder` 和消息构建器，无需实际 CLI 即可进行单元测试
- **跨平台** — macOS、Linux 和 Windows

## 快速开始

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

## 架构

```
┌──────────┐    ┌──────────────┐    ┌───────────┐    ┌─────────────┐    ┌───────────┐
│ 你的代码  │───▶│ ClientConfig │───▶│  Client   │───▶│  Transport  │───▶│ Claude    │
│           │    │              │    │           │    │ (CliTransport│    │ Code CLI  │
│ query()   │    │ model        │    │ connect() │    │  or Mock)   │    │           │
│ query_    │    │ hooks        │    │ send()    │    │             │    │ NDJSON    │
│  stream() │    │ permissions  │    │ close()   │    │ stdin/stdout│───▶│ stdio     │
│ Client    │    │ callbacks    │    │ set_model()│   │ NDJSON      │    │ 协议      │
└──────────┘    └──────────────┘    └───────────┘    └─────────────┘    └───────────┘
                                          │                                    │
                                          ▼                                    ▼
                                    后台任务                        Claude API (Anthropic)
                                    路由: hooks,
                                    permissions,
                                    callbacks
```

## 示例

所有示例均可通过 `cargo run --example <名称>` 运行，需要安装 Claude Code CLI。

| 示例 | 功能 | 运行命令 |
|------|------|---------|
| [`01_basic_query`](examples/01_basic_query.rs) | 单次查询 | `cargo run --example 01_basic_query` |
| [`02_streaming`](examples/02_streaming.rs) | 流式传输 + 模型选择 | `cargo run --example 02_streaming` |
| [`03_multi_turn`](examples/03_multi_turn.rs) | 多轮会话 | `cargo run --example 03_multi_turn` |
| [`04_permissions`](examples/04_permissions.rs) | 权限回调 | `cargo run --example 04_permissions` |
| [`05_hooks`](examples/05_hooks.rs) | 生命周期钩子（3 个事件） | `cargo run --example 05_hooks` |
| [`06_multimodal`](examples/06_multimodal.rs) | 图片 + 文本输入 | `cargo run --example 06_multimodal` |
| [`07_thinking_and_fallback`](examples/07_thinking_and_fallback.rs) | 扩展思维 + 备用模型 | `cargo run --example 07_thinking_and_fallback` |
| [`08_cancellation`](examples/08_cancellation.rs) | 协作式取消 | `cargo run --example 08_cancellation` |
| [`09_message_callback`](examples/09_message_callback.rs) | 消息观察 + 标准错误调试 | `cargo run --example 09_message_callback` |
| [`10_dynamic_control`](examples/10_dynamic_control.rs) | 运行时模型切换 + MCP 服务器 | `cargo run --example 10_dynamic_control` |

## 核心 API

### 单次查询

```rust
use claude_cli_sdk::{query, ClientConfig};

let config = ClientConfig::builder()
    .prompt("List the files in /tmp")
    .build();

let messages = query(config).await?;
```

### 流式传输

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

### 多轮会话

```rust
use claude_cli_sdk::{Client, ClientConfig, Message};
use tokio_stream::StreamExt;

let config = ClientConfig::builder()
    .prompt("Start a Rust project")
    .build();

let mut client = Client::new(config)?;
let session_info = client.connect().await?;
println!("Session: {}", session_info.session_id);

// 第一轮（通过 config 发送提示）
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

// 后续轮次
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

## 高级功能

### 权限回调

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

### 生命周期钩子

注册 8 个生命周期事件的回调：

| 事件 | 触发时机 |
|------|---------|
| `PreToolUse` | 工具执行前 — 可以允许、阻止、修改输入或中止 |
| `PostToolUse` | 工具成功完成后 |
| `PostToolUseFailure` | 工具执行出错后 |
| `UserPromptSubmit` | 用户提交提示时 |
| `Stop` | 智能体会话停止时 |
| `SubagentStop` | 子智能体会话停止时 |
| `PreCompact` | 上下文压缩前 |
| `Notification` | 通用通知事件 |

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

### 多模态（图片）

```rust
use claude_cli_sdk::{query_with_content, ClientConfig, UserContent};

let content = vec![
    UserContent::image_url("https://example.com/diagram.png", "image/png")?,
    UserContent::text("Describe this diagram"),
];
let messages = query_with_content(content, config).await?;
```

也支持通过 `UserContent::image_base64()` 发送 base64 编码的图片。接受的 MIME 类型：`image/jpeg`、`image/png`、`image/gif`、`image/webp`。最大 base64 负载：15 MiB。

### 扩展思维

```rust
let config = ClientConfig::builder()
    .prompt("Solve this step by step")
    .max_thinking_tokens(10_000_u32)
    .build();
```

思维块以 `ContentBlock::Thinking` 形式出现在响应中：

```rust
use claude_cli_sdk::ContentBlock;

for block in &assistant.message.content {
    if let ContentBlock::Thinking(t) = block {
        println!("Thinking: {}", t.thinking);
    }
}
```

### 备用模型

```rust
let config = ClientConfig::builder()
    .prompt("Complex task")
    .model("claude-sonnet-4-5")
    .fallback_model("claude-haiku-4-5")
    .build();
```

### 动态控制

会话中途更改模型或权限模式：

```rust
// 对话中切换模型
client.set_model(Some("claude-haiku-4-5")).await?;

// 恢复为会话默认模型
client.set_model(None).await?;

// 更改权限模式
client.set_permission_mode(PermissionMode::AcceptEdits).await?;

// 发送中断信号 (SIGINT)
client.interrupt().await?;
```

### 消息回调

在消息到达代码前进行观察、转换或过滤：

```rust
use claude_cli_sdk::{ClientConfig, Message, MessageCallback};
use std::sync::Arc;

let callback: MessageCallback = Arc::new(|msg: Message| {
    eprintln!("received: {msg:?}");
    Some(msg) // 透传（返回 None 可过滤掉）
});

let config = ClientConfig::builder()
    .prompt("Hello")
    .message_callback(callback)
    .build();
```

### 取消

```rust
use claude_cli_sdk::{query_stream, CancellationToken, ClientConfig};

let token = CancellationToken::new();
let config = ClientConfig::builder()
    .prompt("Long task")
    .cancellation_token(token.clone())
    .build();

// 从另一个任务取消：
token.cancel();

// 流将产生 Error::Cancelled，可通过以下方式检查：
if error.is_cancelled() { /* 优雅处理 */ }
```

### MCP 服务器

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

### 标准错误调试

捕获 CLI 的标准错误输出用于诊断：

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

## `ClientConfig` 参考

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `prompt` | `String` | **必填** | 初始提示文本 |
| `cli_path` | `Option<PathBuf>` | `None` | CLI 二进制文件路径；`None` 时自动发现 |
| `model` | `Option<String>` | `None` | 模型名称，如 `"claude-sonnet-4-5"` |
| `fallback_model` | `Option<String>` | `None` | 主模型不可用时的备用模型 |
| `cwd` | `Option<PathBuf>` | `None` | CLI 进程的工作目录 |
| `max_turns` | `Option<u32>` | `None` | 停止前的最大智能体轮次 |
| `max_budget_usd` | `Option<f64>` | `None` | 会话费用上限 |
| `max_thinking_tokens` | `Option<u32>` | `None` | 最大扩展思维 token 数 |
| `permission_mode` | `PermissionMode` | `Default` | `Default`、`AcceptEdits`、`Plan` 或 `BypassPermissions` |
| `can_use_tool` | `Option<CanUseToolCallback>` | `None` | 逐工具权限回调 |
| `system_prompt` | `Option<SystemPrompt>` | `None` | 文本或预设系统提示 |
| `allowed_tools` | `Vec<String>` | `[]` | 工具白名单 |
| `disallowed_tools` | `Vec<String>` | `[]` | 工具黑名单 |
| `mcp_servers` | `McpServers` | `{}` | 外部 MCP 服务器定义 |
| `hooks` | `Vec<HookMatcher>` | `[]` | 生命周期钩子注册 |
| `message_callback` | `Option<MessageCallback>` | `None` | 消息观察/过滤回调 |
| `resume` | `Option<String>` | `None` | 通过 ID 恢复已有会话 |
| `verbose` | `bool` | `false` | 启用详细 CLI 输出 |
| `cancellation_token` | `Option<CancellationToken>` | `None` | 协作式取消令牌 |
| `stderr_callback` | `Option<Arc<dyn Fn(String)>>` | `None` | 标准错误输出回调 |
| `connect_timeout` | `Option<Duration>` | `30s` | 启动 + 初始化超时 |
| `close_timeout` | `Option<Duration>` | `10s` | 优雅关闭超时 |
| `read_timeout` | `Option<Duration>` | `None` | 单条消息接收超时 |
| `end_input_on_connect` | `bool` | `true` | 启动后关闭 stdin（`--print` 模式） |
| `default_hook_timeout` | `Duration` | `30s` | 钩子回调默认超时 |
| `version_check_timeout` | `Option<Duration>` | `5s` | `--version` 检查超时 |
| `control_request_timeout` | `Duration` | `30s` | 控制请求超时 |

将任何 `Option<Duration>` 超时设为 `None` 可无限等待。

## Python SDK 对比

| 能力 | Python SDK (`claude-code-sdk`) | 本 crate (`claude-cli-sdk`) |
|------|-------------------------------|------------------------------|
| 单次查询 | `query()` | `query()` |
| 流式传输 | `query_stream()` | `query_stream()` |
| 多轮会话 | `ClaudeCodeSession` | `Client` |
| 权限回调 | `can_use_tool` | `CanUseToolCallback` |
| 生命周期钩子 | `hooks` | `HookMatcher`（8 个事件） |
| MCP 服务器 | `mcp_servers` | `McpServers` |
| 多模态（图片） | `Content` blocks | `UserContent` + `query_with_content()` |
| 扩展思维 | `max_thinking_tokens` | `max_thinking_tokens` |
| 备用模型 | `fallback_model` | `fallback_model` |
| 动态模型切换 | — | `Client::set_model()` |
| 动态权限模式 | — | `Client::set_permission_mode()` |
| 协作式取消 | — | `CancellationToken` |
| 消息回调 | — | `MessageCallback` |
| 标准错误回调 | — | `stderr_callback` |
| 测试框架 | — | `MockTransport` + `ScenarioBuilder` |
| 类型安全 | 运行时 | 编译时（typed builder） |

## 测试

启用 `testing` 特性以在无实际 CLI 的情况下进行单元测试：

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

`ScenarioBuilder` 预先排列 init + exchange 消息，让你的测试在不启动 CLI 进程的情况下执行真实的 `Client` 逻辑。

## 故障排除

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| `CliNotFound` 错误 | Claude Code CLI 不在 `PATH` 中 | 安装：`npm install -g @anthropic-ai/claude-code` |
| `connect()` 超时 | CLI 启动慢或无响应 | 增加 `connect_timeout` 或检查 CLI 安装 |
| 会话在权限请求时挂起 | 未设置 `can_use_tool` 回调但 CLI 请求权限 | 设置 `can_use_tool` 或在 CI 中使用 `PermissionMode::BypassPermissions` |
| "Client dropped without calling close()" 警告 | `Client` 在 `close()` 前被丢弃 | 在丢弃前调用 `client.close().await`，或使用作用域块 |
| 嘈杂的标准错误输出 | CLI 将调试信息打印到 stderr | 设置 `stderr_callback` 捕获/过滤，或不使用 `verbose(true)` |
| `VersionMismatch` 错误 | CLI 版本低于 SDK 最低要求 | 更新 CLI：`npm update -g @anthropic-ai/claude-code` |

## 特性标志

| 特性 | 描述 |
|------|------|
| `testing` | `MockTransport`、`ScenarioBuilder` 和消息构建器辅助，用于单元测试 |
| `efficiency` | 为未来吞吐量优化预留 |
| `integration` | 集成测试辅助（需要实际 CLI） |

## 平台支持

macOS、Linux 和 Windows。

## 免责声明

这是一个非官方的社区开发 SDK，与 Anthropic, PBC 没有关联、背书或赞助关系。"Claude" 和 "Claude Code" 是 Anthropic 的商标。本 crate 与 Claude Code CLI 交互，但不包含任何 Anthropic 专有代码。

## 许可证

可选择 [Apache License, Version 2.0](LICENSE-APACHE) 或 [MIT License](LICENSE-MIT) 中的任一许可证。
