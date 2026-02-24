# claude-cli-sdk

[English](README.md) | [简体中文](README_zh-CN.md) | **日本語** | [한국어](README_ko.md) | [Español](README_es.md)

[![Crates.io](https://img.shields.io/crates/v/claude-cli-sdk.svg)](https://crates.io/crates/claude-cli-sdk)
[![docs.rs](https://docs.rs/claude-cli-sdk/badge.svg)](https://docs.rs/claude-cli-sdk)
[![CI](https://github.com/pomdotdev/claude-cli-sdk/actions/workflows/ci.yml/badge.svg)](https://github.com/pomdotdev/claude-cli-sdk/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#ライセンス)

[Claude Code CLI](https://www.anthropic.com/claude-code) 上でエージェントを構築するための、強い型付けの非同期ファースト Rust SDK。

## 機能

- **ワンショットクエリ** — `query()` でプロンプトを送信し、すべてのレスポンスメッセージを収集
- **ストリーミング** — `query_stream()` でメッセージをリアルタイムに出力
- **マルチターンセッション** — `Client` の `send()` / `receive_messages()` で永続的な会話を実現
- **マルチモーダル入力** — `query_with_content()` で画像（URL または base64）とテキストを送信
- **パーミッションコールバック** — `CanUseToolCallback` でツールごとのプログラム的な承認/拒否
- **8つのライフサイクルフック** — `PreToolUse`、`PostToolUse`、`PostToolUseFailure`、`UserPromptSubmit`、`Stop`、`SubagentStop`、`PreCompact`、`Notification`
- **拡張思考** — `max_thinking_tokens` で思考連鎖推論を有効化
- **フォールバックモデル** — `fallback_model` で自動モデルフェイルオーバー
- **動的制御** — セッション中に `set_model()` と `set_permission_mode()` を呼び出し
- **協調的キャンセル** — `CancellationToken` で優雅な早期終了
- **メッセージコールバック** — メッセージがコードに到達する前に観察、変換、またはフィルタリング
- **標準エラーコールバック** — ログ/診断用に CLI デバッグ出力をキャプチャ
- **MCP サーバー設定** — 外部 Model Context Protocol サーバーを接続
- **テストフレームワーク** — `MockTransport`、`ScenarioBuilder`、メッセージビルダーで実際の CLI なしにユニットテスト
- **クロスプラットフォーム** — macOS、Linux、Windows

## クイックスタート

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

## アーキテクチャ

```
┌──────────┐    ┌──────────────┐    ┌───────────┐    ┌─────────────┐    ┌───────────┐
│ あなたの  │───▶│ ClientConfig │───▶│  Client   │───▶│  Transport  │───▶│ Claude    │
│ コード    │    │              │    │           │    │ (CliTransport│    │ Code CLI  │
│ query()   │    │ model        │    │ connect() │    │  or Mock)   │    │           │
│ query_    │    │ hooks        │    │ send()    │    │             │    │ NDJSON    │
│  stream() │    │ permissions  │    │ close()   │    │ stdin/stdout│───▶│ stdio     │
│ Client    │    │ callbacks    │    │ set_model()│   │ NDJSON      │    │ プロトコル│
└──────────┘    └──────────────┘    └───────────┘    └─────────────┘    └───────────┘
                                          │                                    │
                                          ▼                                    ▼
                                    バックグラウンドタスク          Claude API (Anthropic)
                                    ルーティング: hooks,
                                    permissions,
                                    callbacks
```

## サンプル

すべてのサンプルは `cargo run --example <名前>` で実行できます。Claude Code CLI のインストールが必要です。

| サンプル | 機能 | 実行コマンド |
|---------|------|------------|
| [`01_basic_query`](examples/01_basic_query.rs) | ワンショットクエリ | `cargo run --example 01_basic_query` |
| [`02_streaming`](examples/02_streaming.rs) | ストリーミング + モデル選択 | `cargo run --example 02_streaming` |
| [`03_multi_turn`](examples/03_multi_turn.rs) | マルチターンセッション | `cargo run --example 03_multi_turn` |
| [`04_permissions`](examples/04_permissions.rs) | パーミッションコールバック | `cargo run --example 04_permissions` |
| [`05_hooks`](examples/05_hooks.rs) | ライフサイクルフック（3イベント） | `cargo run --example 05_hooks` |
| [`06_multimodal`](examples/06_multimodal.rs) | 画像 + テキスト入力 | `cargo run --example 06_multimodal` |
| [`07_thinking_and_fallback`](examples/07_thinking_and_fallback.rs) | 拡張思考 + フォールバックモデル | `cargo run --example 07_thinking_and_fallback` |
| [`08_cancellation`](examples/08_cancellation.rs) | 協調的キャンセル | `cargo run --example 08_cancellation` |
| [`09_message_callback`](examples/09_message_callback.rs) | メッセージ観察 + 標準エラーデバッグ | `cargo run --example 09_message_callback` |
| [`10_dynamic_control`](examples/10_dynamic_control.rs) | ランタイムモデル切替 + MCP サーバー | `cargo run --example 10_dynamic_control` |

## コア API

### ワンショットクエリ

```rust
use claude_cli_sdk::{query, ClientConfig};

let config = ClientConfig::builder()
    .prompt("List the files in /tmp")
    .build();

let messages = query(config).await?;
```

### ストリーミング

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

### マルチターンセッション

```rust
use claude_cli_sdk::{Client, ClientConfig, Message};
use tokio_stream::StreamExt;

let config = ClientConfig::builder()
    .prompt("Start a Rust project")
    .build();

let mut client = Client::new(config)?;
let session_info = client.connect().await?;
println!("Session: {}", session_info.session_id);

// 第1ターン（config のプロンプトが送信されます）
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

// 後続ターン
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

## 高度な機能

### パーミッションコールバック

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

### ライフサイクルフック

8つのライフサイクルイベントにコールバックを登録：

| イベント | 発火タイミング |
|---------|--------------|
| `PreToolUse` | ツール実行前 — 許可、ブロック、入力変更、中止が可能 |
| `PostToolUse` | ツール正常完了後 |
| `PostToolUseFailure` | ツールがエラーで失敗した後 |
| `UserPromptSubmit` | ユーザープロンプト送信時 |
| `Stop` | エージェントセッション停止時 |
| `SubagentStop` | サブエージェントセッション停止時 |
| `PreCompact` | コンテキスト圧縮前 |
| `Notification` | 汎用通知イベント |

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

### マルチモーダル（画像）

```rust
use claude_cli_sdk::{query_with_content, ClientConfig, UserContent};

let content = vec![
    UserContent::image_url("https://example.com/diagram.png", "image/png")?,
    UserContent::text("Describe this diagram"),
];
let messages = query_with_content(content, config).await?;
```

`UserContent::image_base64()` による base64 エンコード画像もサポート。対応 MIME タイプ：`image/jpeg`、`image/png`、`image/gif`、`image/webp`。最大 base64 ペイロード：15 MiB。

### 拡張思考

```rust
let config = ClientConfig::builder()
    .prompt("Solve this step by step")
    .max_thinking_tokens(10_000_u32)
    .build();
```

思考ブロックはレスポンスに `ContentBlock::Thinking` として表示されます：

```rust
use claude_cli_sdk::ContentBlock;

for block in &assistant.message.content {
    if let ContentBlock::Thinking(t) = block {
        println!("Thinking: {}", t.thinking);
    }
}
```

### フォールバックモデル

```rust
let config = ClientConfig::builder()
    .prompt("Complex task")
    .model("claude-sonnet-4-5")
    .fallback_model("claude-haiku-4-5")
    .build();
```

### 動的制御

セッション中にモデルやパーミッションモードを変更：

```rust
// 会話中にモデルを切り替え
client.set_model(Some("claude-haiku-4-5")).await?;

// セッションデフォルトに戻す
client.set_model(None).await?;

// パーミッションモードを変更
client.set_permission_mode(PermissionMode::AcceptEdits).await?;

// 中断シグナルを送信 (SIGINT)
client.interrupt().await?;
```

### メッセージコールバック

メッセージがコードに到達する前に観察、変換、またはフィルタリング：

```rust
use claude_cli_sdk::{ClientConfig, Message, MessageCallback};
use std::sync::Arc;

let callback: MessageCallback = Arc::new(|msg: Message| {
    eprintln!("received: {msg:?}");
    Some(msg) // パススルー（None を返すとフィルタリング）
});

let config = ClientConfig::builder()
    .prompt("Hello")
    .message_callback(callback)
    .build();
```

### キャンセル

```rust
use claude_cli_sdk::{query_stream, CancellationToken, ClientConfig};

let token = CancellationToken::new();
let config = ClientConfig::builder()
    .prompt("Long task")
    .cancellation_token(token.clone())
    .build();

// 別のタスクからキャンセル：
token.cancel();

// ストリームは Error::Cancelled を生成。以下で確認：
if error.is_cancelled() { /* 優雅に処理 */ }
```

### MCP サーバー

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

### 標準エラーデバッグ

CLI の標準エラー出力を診断用にキャプチャ：

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

## `ClientConfig` リファレンス

| フィールド | 型 | デフォルト | 説明 |
|-----------|---|---------|------|
| `prompt` | `String` | **必須** | 初期プロンプトテキスト |
| `cli_path` | `Option<PathBuf>` | `None` | CLI バイナリのパス；`None` の場合は自動検出 |
| `model` | `Option<String>` | `None` | モデル名（例：`"claude-sonnet-4-5"`） |
| `fallback_model` | `Option<String>` | `None` | プライマリモデル不可時のフォールバック |
| `cwd` | `Option<PathBuf>` | `None` | CLI プロセスの作業ディレクトリ |
| `max_turns` | `Option<u32>` | `None` | 停止までの最大エージェントターン数 |
| `max_budget_usd` | `Option<f64>` | `None` | セッションのコスト上限 |
| `max_thinking_tokens` | `Option<u32>` | `None` | 最大拡張思考トークン数 |
| `permission_mode` | `PermissionMode` | `Default` | `Default`、`AcceptEdits`、`Plan`、`BypassPermissions` |
| `can_use_tool` | `Option<CanUseToolCallback>` | `None` | ツールごとのパーミッションコールバック |
| `system_prompt` | `Option<SystemPrompt>` | `None` | テキストまたはプリセットシステムプロンプト |
| `allowed_tools` | `Vec<String>` | `[]` | ツールの許可リスト |
| `disallowed_tools` | `Vec<String>` | `[]` | ツールのブロックリスト |
| `mcp_servers` | `McpServers` | `{}` | 外部 MCP サーバー定義 |
| `hooks` | `Vec<HookMatcher>` | `[]` | ライフサイクルフック登録 |
| `message_callback` | `Option<MessageCallback>` | `None` | メッセージ観察/フィルタコールバック |
| `resume` | `Option<String>` | `None` | ID で既存セッションを再開 |
| `verbose` | `bool` | `false` | 詳細 CLI 出力を有効化 |
| `cancellation_token` | `Option<CancellationToken>` | `None` | 協調的キャンセルトークン |
| `stderr_callback` | `Option<Arc<dyn Fn(String)>>` | `None` | 標準エラー出力コールバック |
| `connect_timeout` | `Option<Duration>` | `30s` | 起動 + 初期化のデッドライン |
| `close_timeout` | `Option<Duration>` | `10s` | 優雅なシャットダウンのデッドライン |
| `read_timeout` | `Option<Duration>` | `None` | メッセージごとの受信デッドライン |
| `end_input_on_connect` | `bool` | `true` | 起動後に stdin を閉じる（`--print` モード） |
| `default_hook_timeout` | `Duration` | `30s` | フックコールバックのフォールバックタイムアウト |
| `version_check_timeout` | `Option<Duration>` | `5s` | `--version` チェックのデッドライン |
| `control_request_timeout` | `Duration` | `30s` | コントロールリクエストのデッドライン |

`Option<Duration>` タイムアウトを `None` に設定すると無制限に待機します。

## Python SDK との比較

| 機能 | Python SDK (`claude-code-sdk`) | 本 crate (`claude-cli-sdk`) |
|------|-------------------------------|------------------------------|
| ワンショットクエリ | `query()` | `query()` |
| ストリーミング | `query_stream()` | `query_stream()` |
| マルチターンセッション | `ClaudeCodeSession` | `Client` |
| パーミッションコールバック | `can_use_tool` | `CanUseToolCallback` |
| ライフサイクルフック | `hooks` | `HookMatcher`（8イベント） |
| MCP サーバー | `mcp_servers` | `McpServers` |
| マルチモーダル（画像） | `Content` blocks | `UserContent` + `query_with_content()` |
| 拡張思考 | `max_thinking_tokens` | `max_thinking_tokens` |
| フォールバックモデル | `fallback_model` | `fallback_model` |
| 動的モデル切替 | — | `Client::set_model()` |
| 動的パーミッションモード | — | `Client::set_permission_mode()` |
| 協調的キャンセル | — | `CancellationToken` |
| メッセージコールバック | — | `MessageCallback` |
| 標準エラーコールバック | — | `stderr_callback` |
| テストフレームワーク | — | `MockTransport` + `ScenarioBuilder` |
| 型安全性 | 実行時 | コンパイル時（typed builder） |

## テスト

`testing` フィーチャーを有効にすると、実際の CLI なしでユニットテストが可能です：

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

`ScenarioBuilder` は init + exchange メッセージを事前にキューイングし、CLI プロセスを起動せずに実際の `Client` ロジックをテストできます。

## トラブルシューティング

| 問題 | 原因 | 解決方法 |
|------|------|---------|
| `CliNotFound` エラー | Claude Code CLI が `PATH` にない | インストール：`npm install -g @anthropic-ai/claude-code` |
| `connect()` タイムアウト | CLI の起動が遅いか無応答 | `connect_timeout` を増やすか CLI のインストールを確認 |
| パーミッションリクエストでセッションがハング | `can_use_tool` コールバック未設定で CLI がパーミッションを要求 | `can_use_tool` を設定するか CI で `PermissionMode::BypassPermissions` を使用 |
| "Client dropped without calling close()" 警告 | `close()` 前に `Client` がドロップ | ドロップ前に `client.close().await` を呼ぶか、スコープブロックを使用 |
| 標準エラーの出力ノイズ | CLI がデバッグ情報を stderr に出力 | `stderr_callback` でキャプチャ/フィルタするか `verbose(true)` を省略 |
| `VersionMismatch` エラー | CLI バージョンが SDK の最低要件を下回る | CLI を更新：`npm update -g @anthropic-ai/claude-code` |

## フィーチャーフラグ

| フィーチャー | 説明 |
|------------|------|
| `testing` | `MockTransport`、`ScenarioBuilder`、メッセージビルダーヘルパー（ユニットテスト用） |
| `efficiency` | 将来のスループット最適化用に予約 |
| `integration` | 統合テストヘルパー（実際の CLI が必要） |

## プラットフォームサポート

macOS、Linux、Windows。

## 免責事項

これは非公式のコミュニティ開発 SDK であり、Anthropic, PBC とは提携、推薦、スポンサー関係にありません。「Claude」および「Claude Code」は Anthropic の商標です。本 crate は Claude Code CLI と連携しますが、Anthropic の独自コードは含まれていません。

## ライセンス

[Apache License, Version 2.0](LICENSE-APACHE) または [MIT License](LICENSE-MIT) のいずれかを選択できます。
