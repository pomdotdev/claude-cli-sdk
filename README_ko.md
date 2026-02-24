# claude-cli-sdk

[English](README.md) | [简体中文](README_zh-CN.md) | [日本語](README_ja.md) | **한국어** | [Español](README_es.md)

[![Crates.io](https://img.shields.io/crates/v/claude-cli-sdk.svg)](https://crates.io/crates/claude-cli-sdk)
[![docs.rs](https://docs.rs/claude-cli-sdk/badge.svg)](https://docs.rs/claude-cli-sdk)
[![CI](https://github.com/pomdotdev/claude-cli-sdk/actions/workflows/ci.yml/badge.svg)](https://github.com/pomdotdev/claude-cli-sdk/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#라이선스)

[Claude Code CLI](https://www.anthropic.com/claude-code) 기반으로 에이전트를 구축하기 위한 강타입, 비동기 우선 Rust SDK.

## 기능

- **원샷 쿼리** — `query()`로 프롬프트를 보내고 모든 응답 메시지를 수집
- **스트리밍** — `query_stream()`으로 메시지를 실시간 출력
- **멀티턴 세션** — `Client`의 `send()` / `receive_messages()`로 영구 대화 지원
- **멀티모달 입력** — `query_with_content()`로 이미지(URL 또는 base64)와 텍스트 전송
- **권한 콜백** — `CanUseToolCallback`으로 도구별 프로그래밍 방식 승인/거부
- **8개 라이프사이클 훅** — `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `UserPromptSubmit`, `Stop`, `SubagentStop`, `PreCompact`, `Notification`
- **확장 사고** — `max_thinking_tokens`로 사고 연쇄 추론 활성화
- **대체 모델** — `fallback_model`로 자동 모델 장애 조치
- **동적 제어** — 세션 중 `set_model()` 및 `set_permission_mode()` 호출
- **협력적 취소** — `CancellationToken`으로 우아한 조기 종료
- **메시지 콜백** — 메시지가 코드에 도달하기 전 관찰, 변환 또는 필터링
- **표준 에러 콜백** — 로깅/진단을 위한 CLI 디버그 출력 캡처
- **MCP 서버 설정** — 외부 Model Context Protocol 서버 연결
- **테스트 프레임워크** — `MockTransport`, `ScenarioBuilder`, 메시지 빌더로 실제 CLI 없이 유닛 테스트
- **크로스 플랫폼** — macOS, Linux, Windows

## 빠른 시작

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

## 아키텍처

```
┌──────────┐    ┌──────────────┐    ┌───────────┐    ┌─────────────┐    ┌───────────┐
│ 사용자    │───▶│ ClientConfig │───▶│  Client   │───▶│  Transport  │───▶│ Claude    │
│ 코드      │    │              │    │           │    │ (CliTransport│    │ Code CLI  │
│ query()   │    │ model        │    │ connect() │    │  or Mock)   │    │           │
│ query_    │    │ hooks        │    │ send()    │    │             │    │ NDJSON    │
│  stream() │    │ permissions  │    │ close()   │    │ stdin/stdout│───▶│ stdio     │
│ Client    │    │ callbacks    │    │ set_model()│   │ NDJSON      │    │ 프로토콜  │
└──────────┘    └──────────────┘    └───────────┘    └─────────────┘    └───────────┘
                                          │                                    │
                                          ▼                                    ▼
                                    백그라운드 태스크              Claude API (Anthropic)
                                    라우팅: hooks,
                                    permissions,
                                    callbacks
```

## 예제

모든 예제는 `cargo run --example <이름>`으로 실행할 수 있습니다. Claude Code CLI 설치가 필요합니다.

| 예제 | 기능 | 실행 명령 |
|------|------|----------|
| [`01_basic_query`](examples/01_basic_query.rs) | 원샷 쿼리 | `cargo run --example 01_basic_query` |
| [`02_streaming`](examples/02_streaming.rs) | 스트리밍 + 모델 선택 | `cargo run --example 02_streaming` |
| [`03_multi_turn`](examples/03_multi_turn.rs) | 멀티턴 세션 | `cargo run --example 03_multi_turn` |
| [`04_permissions`](examples/04_permissions.rs) | 권한 콜백 | `cargo run --example 04_permissions` |
| [`05_hooks`](examples/05_hooks.rs) | 라이프사이클 훅 (3개 이벤트) | `cargo run --example 05_hooks` |
| [`06_multimodal`](examples/06_multimodal.rs) | 이미지 + 텍스트 입력 | `cargo run --example 06_multimodal` |
| [`07_thinking_and_fallback`](examples/07_thinking_and_fallback.rs) | 확장 사고 + 대체 모델 | `cargo run --example 07_thinking_and_fallback` |
| [`08_cancellation`](examples/08_cancellation.rs) | 협력적 취소 | `cargo run --example 08_cancellation` |
| [`09_message_callback`](examples/09_message_callback.rs) | 메시지 관찰 + 표준 에러 디버그 | `cargo run --example 09_message_callback` |
| [`10_dynamic_control`](examples/10_dynamic_control.rs) | 런타임 모델 전환 + MCP 서버 | `cargo run --example 10_dynamic_control` |

## 핵심 API

### 원샷 쿼리

```rust
use claude_cli_sdk::{query, ClientConfig};

let config = ClientConfig::builder()
    .prompt("List the files in /tmp")
    .build();

let messages = query(config).await?;
```

### 스트리밍

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

### 멀티턴 세션

```rust
use claude_cli_sdk::{Client, ClientConfig, Message};
use tokio_stream::StreamExt;

let config = ClientConfig::builder()
    .prompt("Start a Rust project")
    .build();

let mut client = Client::new(config)?;
let session_info = client.connect().await?;
println!("Session: {}", session_info.session_id);

// 첫 번째 턴 (config의 프롬프트가 전송됨)
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

// 후속 턴
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

## 고급 기능

### 권한 콜백

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

### 라이프사이클 훅

8개 라이프사이클 이벤트에 콜백을 등록:

| 이벤트 | 발생 시점 |
|--------|----------|
| `PreToolUse` | 도구 실행 전 — 허용, 차단, 입력 수정 또는 중단 가능 |
| `PostToolUse` | 도구 성공적 완료 후 |
| `PostToolUseFailure` | 도구 에러 실패 후 |
| `UserPromptSubmit` | 사용자 프롬프트 제출 시 |
| `Stop` | 에이전트 세션 중지 시 |
| `SubagentStop` | 서브에이전트 세션 중지 시 |
| `PreCompact` | 컨텍스트 압축 전 |
| `Notification` | 일반 알림 이벤트 |

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

### 멀티모달 (이미지)

```rust
use claude_cli_sdk::{query_with_content, ClientConfig, UserContent};

let content = vec![
    UserContent::image_url("https://example.com/diagram.png", "image/png")?,
    UserContent::text("Describe this diagram"),
];
let messages = query_with_content(content, config).await?;
```

`UserContent::image_base64()`를 통한 base64 인코딩 이미지도 지원합니다. 허용 MIME 타입: `image/jpeg`, `image/png`, `image/gif`, `image/webp`. 최대 base64 페이로드: 15 MiB.

### 확장 사고

```rust
let config = ClientConfig::builder()
    .prompt("Solve this step by step")
    .max_thinking_tokens(10_000_u32)
    .build();
```

사고 블록은 응답에서 `ContentBlock::Thinking`으로 나타납니다:

```rust
use claude_cli_sdk::ContentBlock;

for block in &assistant.message.content {
    if let ContentBlock::Thinking(t) = block {
        println!("Thinking: {}", t.thinking);
    }
}
```

### 대체 모델

```rust
let config = ClientConfig::builder()
    .prompt("Complex task")
    .model("claude-sonnet-4-5")
    .fallback_model("claude-haiku-4-5")
    .build();
```

### 동적 제어

세션 중 모델이나 권한 모드 변경:

```rust
// 대화 중 모델 전환
client.set_model(Some("claude-haiku-4-5")).await?;

// 세션 기본값으로 복원
client.set_model(None).await?;

// 권한 모드 변경
client.set_permission_mode(PermissionMode::AcceptEdits).await?;

// 인터럽트 시그널 전송 (SIGINT)
client.interrupt().await?;
```

### 메시지 콜백

메시지가 코드에 도달하기 전 관찰, 변환 또는 필터링:

```rust
use claude_cli_sdk::{ClientConfig, Message, MessageCallback};
use std::sync::Arc;

let callback: MessageCallback = Arc::new(|msg: Message| {
    eprintln!("received: {msg:?}");
    Some(msg) // 통과 (None을 반환하면 필터링)
});

let config = ClientConfig::builder()
    .prompt("Hello")
    .message_callback(callback)
    .build();
```

### 취소

```rust
use claude_cli_sdk::{query_stream, CancellationToken, ClientConfig};

let token = CancellationToken::new();
let config = ClientConfig::builder()
    .prompt("Long task")
    .cancellation_token(token.clone())
    .build();

// 다른 태스크에서 취소:
token.cancel();

// 스트림은 Error::Cancelled를 생성하며 다음으로 확인:
if error.is_cancelled() { /* 우아하게 처리 */ }
```

### MCP 서버

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

### 표준 에러 디버깅

CLI의 표준 에러 출력을 진단용으로 캡처:

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

## `ClientConfig` 레퍼런스

| 필드 | 타입 | 기본값 | 설명 |
|------|------|--------|------|
| `prompt` | `String` | **필수** | 초기 프롬프트 텍스트 |
| `cli_path` | `Option<PathBuf>` | `None` | CLI 바이너리 경로; `None`이면 자동 탐색 |
| `model` | `Option<String>` | `None` | 모델 이름 (예: `"claude-sonnet-4-5"`) |
| `fallback_model` | `Option<String>` | `None` | 기본 모델 사용 불가 시 대체 모델 |
| `cwd` | `Option<PathBuf>` | `None` | CLI 프로세스의 작업 디렉토리 |
| `max_turns` | `Option<u32>` | `None` | 중지 전 최대 에이전트 턴 수 |
| `max_budget_usd` | `Option<f64>` | `None` | 세션 비용 상한 |
| `max_thinking_tokens` | `Option<u32>` | `None` | 최대 확장 사고 토큰 수 |
| `permission_mode` | `PermissionMode` | `Default` | `Default`, `AcceptEdits`, `Plan`, `BypassPermissions` |
| `can_use_tool` | `Option<CanUseToolCallback>` | `None` | 도구별 권한 콜백 |
| `system_prompt` | `Option<SystemPrompt>` | `None` | 텍스트 또는 프리셋 시스템 프롬프트 |
| `allowed_tools` | `Vec<String>` | `[]` | 도구 허용 목록 |
| `disallowed_tools` | `Vec<String>` | `[]` | 도구 차단 목록 |
| `mcp_servers` | `McpServers` | `{}` | 외부 MCP 서버 정의 |
| `hooks` | `Vec<HookMatcher>` | `[]` | 라이프사이클 훅 등록 |
| `message_callback` | `Option<MessageCallback>` | `None` | 메시지 관찰/필터 콜백 |
| `resume` | `Option<String>` | `None` | ID로 기존 세션 재개 |
| `verbose` | `bool` | `false` | 상세 CLI 출력 활성화 |
| `cancellation_token` | `Option<CancellationToken>` | `None` | 협력적 취소 토큰 |
| `stderr_callback` | `Option<Arc<dyn Fn(String)>>` | `None` | 표준 에러 출력 콜백 |
| `connect_timeout` | `Option<Duration>` | `30s` | 시작 + 초기화 데드라인 |
| `close_timeout` | `Option<Duration>` | `10s` | 우아한 종료 데드라인 |
| `read_timeout` | `Option<Duration>` | `None` | 메시지별 수신 데드라인 |
| `end_input_on_connect` | `bool` | `true` | 시작 후 stdin 닫기 (`--print` 모드) |
| `default_hook_timeout` | `Duration` | `30s` | 훅 콜백 폴백 타임아웃 |
| `version_check_timeout` | `Option<Duration>` | `5s` | `--version` 확인 데드라인 |
| `control_request_timeout` | `Duration` | `30s` | 제어 요청 데드라인 |

`Option<Duration>` 타임아웃을 `None`으로 설정하면 무기한 대기합니다.

## Python SDK 비교

| 기능 | Python SDK (`claude-code-sdk`) | 이 crate (`claude-cli-sdk`) |
|------|-------------------------------|------------------------------|
| 원샷 쿼리 | `query()` | `query()` |
| 스트리밍 | `query_stream()` | `query_stream()` |
| 멀티턴 세션 | `ClaudeCodeSession` | `Client` |
| 권한 콜백 | `can_use_tool` | `CanUseToolCallback` |
| 라이프사이클 훅 | `hooks` | `HookMatcher` (8개 이벤트) |
| MCP 서버 | `mcp_servers` | `McpServers` |
| 멀티모달 (이미지) | `Content` blocks | `UserContent` + `query_with_content()` |
| 확장 사고 | `max_thinking_tokens` | `max_thinking_tokens` |
| 대체 모델 | `fallback_model` | `fallback_model` |
| 동적 모델 전환 | — | `Client::set_model()` |
| 동적 권한 모드 | — | `Client::set_permission_mode()` |
| 협력적 취소 | — | `CancellationToken` |
| 메시지 콜백 | — | `MessageCallback` |
| 표준 에러 콜백 | — | `stderr_callback` |
| 테스트 프레임워크 | — | `MockTransport` + `ScenarioBuilder` |
| 타입 안전성 | 런타임 | 컴파일 타임 (typed builder) |

## 테스트

`testing` 피처를 활성화하면 실제 CLI 없이 유닛 테스트가 가능합니다:

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

`ScenarioBuilder`는 init + exchange 메시지를 미리 큐에 넣어 CLI 프로세스를 시작하지 않고 실제 `Client` 로직을 테스트할 수 있습니다.

## 문제 해결

| 문제 | 원인 | 해결 방법 |
|------|------|----------|
| `CliNotFound` 에러 | Claude Code CLI가 `PATH`에 없음 | 설치: `npm install -g @anthropic-ai/claude-code` |
| `connect()` 타임아웃 | CLI 시작이 느리거나 응답 없음 | `connect_timeout` 증가 또는 CLI 설치 확인 |
| 권한 요청 시 세션 멈춤 | `can_use_tool` 콜백 미설정인데 CLI가 권한 요청 | `can_use_tool` 설정 또는 CI에서 `PermissionMode::BypassPermissions` 사용 |
| "Client dropped without calling close()" 경고 | `close()` 전에 `Client`가 드롭됨 | 드롭 전 `client.close().await` 호출 또는 스코프 블록 사용 |
| 표준 에러 출력 노이즈 | CLI가 디버그 정보를 stderr로 출력 | `stderr_callback`으로 캡처/필터 또는 `verbose(true)` 생략 |
| `VersionMismatch` 에러 | CLI 버전이 SDK 최소 요구사항 미만 | CLI 업데이트: `npm update -g @anthropic-ai/claude-code` |

## 피처 플래그

| 피처 | 설명 |
|------|------|
| `testing` | `MockTransport`, `ScenarioBuilder`, 메시지 빌더 헬퍼 (유닛 테스트용) |
| `efficiency` | 향후 처리량 최적화를 위해 예약 |
| `integration` | 통합 테스트 헬퍼 (실제 CLI 필요) |

## 플랫폼 지원

macOS, Linux, Windows.

## 면책 조항

이것은 비공식 커뮤니티 개발 SDK이며 Anthropic, PBC와 제휴, 보증 또는 후원 관계가 없습니다. "Claude" 및 "Claude Code"는 Anthropic의 상표입니다. 이 crate는 Claude Code CLI와 상호작용하지만 Anthropic 독점 코드를 포함하지 않습니다.

## 라이선스

[Apache License, Version 2.0](LICENSE-APACHE) 또는 [MIT License](LICENSE-MIT) 중 선택할 수 있습니다.
