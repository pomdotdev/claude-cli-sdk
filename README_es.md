# claude-cli-sdk

[English](README.md) | [简体中文](README_zh-CN.md) | [日本語](README_ja.md) | [한국어](README_ko.md) | **Español**

[![Crates.io](https://img.shields.io/crates/v/claude-cli-sdk.svg)](https://crates.io/crates/claude-cli-sdk)
[![docs.rs](https://docs.rs/claude-cli-sdk/badge.svg)](https://docs.rs/claude-cli-sdk)
[![CI](https://github.com/pomdotdev/claude-cli-sdk/actions/workflows/ci.yml/badge.svg)](https://github.com/pomdotdev/claude-cli-sdk/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#licencia)

SDK de Rust fuertemente tipado y async-first para construir agentes sobre [Claude Code CLI](https://www.anthropic.com/claude-code).

## Funcionalidades

- **Consultas de un solo paso** — `query()` envía un prompt y recopila todos los mensajes de respuesta
- **Streaming** — `query_stream()` produce mensajes a medida que llegan
- **Sesiones multi-turno** — `Client` con `send()` / `receive_messages()` para conversaciones persistentes
- **Entrada multimodal** — envía imágenes (URL o base64) junto con texto vía `query_with_content()`
- **Callbacks de permisos** — `CanUseToolCallback` para aprobación/rechazo programático por herramienta
- **8 hooks de ciclo de vida** — `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `UserPromptSubmit`, `Stop`, `SubagentStop`, `PreCompact`, `Notification`
- **Pensamiento extendido** — `max_thinking_tokens` para habilitar razonamiento de cadena de pensamiento
- **Modelo de respaldo** — `fallback_model` para failover automático de modelo
- **Control dinámico** — `set_model()` y `set_permission_mode()` durante la sesión
- **Cancelación cooperativa** — `CancellationToken` para terminación temprana elegante
- **Callback de mensajes** — observar, transformar o filtrar mensajes antes de que lleguen a tu código
- **Callback de stderr** — capturar salida de depuración del CLI para logging/diagnóstico
- **Configuración de servidores MCP** — adjuntar servidores externos de Model Context Protocol
- **Framework de testing** — `MockTransport`, `ScenarioBuilder` y constructores de mensajes para tests unitarios sin CLI real
- **Multiplataforma** — macOS, Linux y Windows

## Inicio Rápido

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

## Arquitectura

```
┌──────────┐    ┌──────────────┐    ┌───────────┐    ┌─────────────┐    ┌───────────┐
│ Tu Código │───▶│ ClientConfig │───▶│  Client   │───▶│  Transport  │───▶│ Claude    │
│           │    │              │    │           │    │ (CliTransport│    │ Code CLI  │
│ query()   │    │ model        │    │ connect() │    │  or Mock)   │    │           │
│ query_    │    │ hooks        │    │ send()    │    │             │    │ NDJSON    │
│  stream() │    │ permissions  │    │ close()   │    │ stdin/stdout│───▶│ stdio     │
│ Client    │    │ callbacks    │    │ set_model()│   │ NDJSON      │    │ protocolo │
└──────────┘    └──────────────┘    └───────────┘    └─────────────┘    └───────────┘
                                          │                                    │
                                          ▼                                    ▼
                                    Tarea en segundo           Claude API (Anthropic)
                                    plano: enruta hooks,
                                    permissions,
                                    callbacks
```

## Ejemplos

Todos los ejemplos se pueden ejecutar con `cargo run --example <nombre>`. Requieren una instalación funcional de Claude Code CLI.

| Ejemplo | Funcionalidad | Ejecutar |
|---------|--------------|----------|
| [`01_basic_query`](examples/01_basic_query.rs) | Consulta de un solo paso | `cargo run --example 01_basic_query` |
| [`02_streaming`](examples/02_streaming.rs) | Streaming + selección de modelo | `cargo run --example 02_streaming` |
| [`03_multi_turn`](examples/03_multi_turn.rs) | Sesiones multi-turno | `cargo run --example 03_multi_turn` |
| [`04_permissions`](examples/04_permissions.rs) | Callbacks de permisos | `cargo run --example 04_permissions` |
| [`05_hooks`](examples/05_hooks.rs) | Hooks de ciclo de vida (3 eventos) | `cargo run --example 05_hooks` |
| [`06_multimodal`](examples/06_multimodal.rs) | Imagen + texto | `cargo run --example 06_multimodal` |
| [`07_thinking_and_fallback`](examples/07_thinking_and_fallback.rs) | Pensamiento extendido + modelo de respaldo | `cargo run --example 07_thinking_and_fallback` |
| [`08_cancellation`](examples/08_cancellation.rs) | Cancelación cooperativa | `cargo run --example 08_cancellation` |
| [`09_message_callback`](examples/09_message_callback.rs) | Observación de mensajes + depuración stderr | `cargo run --example 09_message_callback` |
| [`10_dynamic_control`](examples/10_dynamic_control.rs) | Cambio de modelo en tiempo de ejecución + servidores MCP | `cargo run --example 10_dynamic_control` |

## API Principal

### Consulta de un solo paso

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

### Sesiones multi-turno

```rust
use claude_cli_sdk::{Client, ClientConfig, Message};
use tokio_stream::StreamExt;

let config = ClientConfig::builder()
    .prompt("Start a Rust project")
    .build();

let mut client = Client::new(config)?;
let session_info = client.connect().await?;
println!("Session: {}", session_info.session_id);

// Primer turno (el prompt se envía vía config).
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

// Turnos posteriores.
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

## Funcionalidades Avanzadas

### Callbacks de permisos

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

### Hooks de ciclo de vida

Registra callbacks para 8 eventos de ciclo de vida:

| Evento | Cuándo se dispara |
|--------|------------------|
| `PreToolUse` | Antes de ejecutar una herramienta — puede permitir, bloquear, modificar entrada o abortar |
| `PostToolUse` | Después de que una herramienta se complete exitosamente |
| `PostToolUseFailure` | Después de que una herramienta falle con error |
| `UserPromptSubmit` | Cuando se envía un prompt de usuario |
| `Stop` | Cuando la sesión del agente se detiene |
| `SubagentStop` | Cuando la sesión de un subagente se detiene |
| `PreCompact` | Antes de la compactación de contexto |
| `Notification` | Evento de notificación general |

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

### Multimodal (imágenes)

```rust
use claude_cli_sdk::{query_with_content, ClientConfig, UserContent};

let content = vec![
    UserContent::image_url("https://example.com/diagram.png", "image/png")?,
    UserContent::text("Describe this diagram"),
];
let messages = query_with_content(content, config).await?;
```

También soporta imágenes codificadas en base64 vía `UserContent::image_base64()`. Tipos MIME aceptados: `image/jpeg`, `image/png`, `image/gif`, `image/webp`. Payload máximo en base64: 15 MiB.

### Pensamiento extendido

```rust
let config = ClientConfig::builder()
    .prompt("Solve this step by step")
    .max_thinking_tokens(10_000_u32)
    .build();
```

Los bloques de pensamiento aparecen como `ContentBlock::Thinking` en la respuesta:

```rust
use claude_cli_sdk::ContentBlock;

for block in &assistant.message.content {
    if let ContentBlock::Thinking(t) = block {
        println!("Thinking: {}", t.thinking);
    }
}
```

### Modelo de respaldo

```rust
let config = ClientConfig::builder()
    .prompt("Complex task")
    .model("claude-sonnet-4-5")
    .fallback_model("claude-haiku-4-5")
    .build();
```

### Control dinámico

Cambia el modelo o modo de permisos durante la sesión:

```rust
// Cambiar modelo durante una conversación
client.set_model(Some("claude-haiku-4-5")).await?;

// Revertir al modelo predeterminado de la sesión
client.set_model(None).await?;

// Cambiar modo de permisos
client.set_permission_mode(PermissionMode::AcceptEdits).await?;

// Enviar señal de interrupción (SIGINT)
client.interrupt().await?;
```

### Callback de mensajes

Observar, transformar o filtrar mensajes antes de que lleguen a tu código:

```rust
use claude_cli_sdk::{ClientConfig, Message, MessageCallback};
use std::sync::Arc;

let callback: MessageCallback = Arc::new(|msg: Message| {
    eprintln!("received: {msg:?}");
    Some(msg) // pasar (devolver None para filtrar)
});

let config = ClientConfig::builder()
    .prompt("Hello")
    .message_callback(callback)
    .build();
```

### Cancelación

```rust
use claude_cli_sdk::{query_stream, CancellationToken, ClientConfig};

let token = CancellationToken::new();
let config = ClientConfig::builder()
    .prompt("Long task")
    .cancellation_token(token.clone())
    .build();

// Cancelar desde otra tarea:
token.cancel();

// El stream producirá Error::Cancelled, verificable con:
if error.is_cancelled() { /* manejar elegantemente */ }
```

### Servidores MCP

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

### Depuración con stderr

Capturar la salida stderr del CLI para diagnóstico:

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

## Referencia de `ClientConfig`

| Campo | Tipo | Predeterminado | Descripción |
|-------|------|---------------|-------------|
| `prompt` | `String` | **Requerido** | Texto del prompt inicial |
| `cli_path` | `Option<PathBuf>` | `None` | Ruta al binario CLI; si `None`, descubierto automáticamente |
| `model` | `Option<String>` | `None` | Nombre del modelo, ej. `"claude-sonnet-4-5"` |
| `fallback_model` | `Option<String>` | `None` | Respaldo si el modelo primario no está disponible |
| `cwd` | `Option<PathBuf>` | `None` | Directorio de trabajo para el proceso CLI |
| `max_turns` | `Option<u32>` | `None` | Máximo de turnos del agente antes de detenerse |
| `max_budget_usd` | `Option<f64>` | `None` | Límite de costo para la sesión |
| `max_thinking_tokens` | `Option<u32>` | `None` | Máximo de tokens de pensamiento extendido |
| `permission_mode` | `PermissionMode` | `Default` | `Default`, `AcceptEdits`, `Plan` o `BypassPermissions` |
| `can_use_tool` | `Option<CanUseToolCallback>` | `None` | Callback de permisos por herramienta |
| `system_prompt` | `Option<SystemPrompt>` | `None` | Prompt de sistema de texto o preset |
| `allowed_tools` | `Vec<String>` | `[]` | Lista de herramientas permitidas |
| `disallowed_tools` | `Vec<String>` | `[]` | Lista de herramientas bloqueadas |
| `mcp_servers` | `McpServers` | `{}` | Definiciones de servidores MCP externos |
| `hooks` | `Vec<HookMatcher>` | `[]` | Registro de hooks de ciclo de vida |
| `message_callback` | `Option<MessageCallback>` | `None` | Callback de observación/filtrado de mensajes |
| `resume` | `Option<String>` | `None` | Reanudar sesión existente por ID |
| `verbose` | `bool` | `false` | Habilitar salida CLI detallada |
| `cancellation_token` | `Option<CancellationToken>` | `None` | Token de cancelación cooperativa |
| `stderr_callback` | `Option<Arc<dyn Fn(String)>>` | `None` | Callback de salida stderr |
| `connect_timeout` | `Option<Duration>` | `30s` | Tiempo límite para inicio + inicialización |
| `close_timeout` | `Option<Duration>` | `10s` | Tiempo límite para cierre elegante |
| `read_timeout` | `Option<Duration>` | `None` | Tiempo límite de recepción por mensaje |
| `end_input_on_connect` | `bool` | `true` | Cerrar stdin después de iniciar (modo `--print`) |
| `default_hook_timeout` | `Duration` | `30s` | Timeout de respaldo para callbacks de hooks |
| `version_check_timeout` | `Option<Duration>` | `5s` | Tiempo límite para verificación de `--version` |
| `control_request_timeout` | `Duration` | `30s` | Tiempo límite para solicitudes de control |

Establece cualquier timeout `Option<Duration>` a `None` para esperar indefinidamente.

## Comparación con el SDK de Python

| Capacidad | SDK Python (`claude-code-sdk`) | Este crate (`claude-cli-sdk`) |
|-----------|-------------------------------|------------------------------|
| Consulta de un solo paso | `query()` | `query()` |
| Streaming | `query_stream()` | `query_stream()` |
| Sesiones multi-turno | `ClaudeCodeSession` | `Client` |
| Callbacks de permisos | `can_use_tool` | `CanUseToolCallback` |
| Hooks de ciclo de vida | `hooks` | `HookMatcher` (8 eventos) |
| Servidores MCP | `mcp_servers` | `McpServers` |
| Multimodal (imágenes) | bloques `Content` | `UserContent` + `query_with_content()` |
| Pensamiento extendido | `max_thinking_tokens` | `max_thinking_tokens` |
| Modelo de respaldo | `fallback_model` | `fallback_model` |
| Cambio dinámico de modelo | — | `Client::set_model()` |
| Modo de permisos dinámico | — | `Client::set_permission_mode()` |
| Cancelación cooperativa | — | `CancellationToken` |
| Callback de mensajes | — | `MessageCallback` |
| Callback de stderr | — | `stderr_callback` |
| Framework de testing | — | `MockTransport` + `ScenarioBuilder` |
| Seguridad de tipos | Runtime | Tiempo de compilación (typed builder) |

## Testing

Habilita el feature `testing` para tests unitarios sin CLI real:

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

`ScenarioBuilder` encola mensajes init + exchange para que tus tests ejerzan la lógica real de `Client` sin iniciar un proceso CLI.

## Solución de Problemas

| Problema | Causa | Solución |
|----------|-------|---------|
| Error `CliNotFound` | Claude Code CLI no está en `PATH` | Instalar: `npm install -g @anthropic-ai/claude-code` |
| Timeout en `connect()` | CLI lento al iniciar o sin respuesta | Aumentar `connect_timeout` o verificar instalación del CLI |
| Sesión cuelga en solicitud de permisos | Callback `can_use_tool` no configurado pero CLI solicita permiso | Configurar `can_use_tool` o usar `PermissionMode::BypassPermissions` en CI |
| Advertencia "Client dropped without calling close()" | `Client` eliminado antes de `close()` | Llamar `client.close().await` antes de eliminar, o usar bloques de alcance |
| Salida ruidosa en stderr | CLI imprime info de depuración en stderr | Configurar `stderr_callback` para capturar/filtrar, u omitir `verbose(true)` |
| Error `VersionMismatch` | Versión del CLI inferior al mínimo del SDK | Actualizar CLI: `npm update -g @anthropic-ai/claude-code` |

## Feature Flags

| Feature | Descripción |
|---------|-------------|
| `testing` | `MockTransport`, `ScenarioBuilder` y helpers de construcción de mensajes para tests unitarios |
| `efficiency` | Reservado para futuras optimizaciones de rendimiento |
| `integration` | Helpers de tests de integración (requiere CLI real) |

## Soporte de Plataformas

macOS, Linux y Windows.

## Aviso Legal

Este es un SDK no oficial desarrollado por la comunidad y no está afiliado, respaldado ni patrocinado por Anthropic, PBC. "Claude" y "Claude Code" son marcas registradas de Anthropic. Este crate interactúa con Claude Code CLI pero no contiene código propietario de Anthropic.

## Licencia

Licenciado bajo cualquiera de [Apache License, Version 2.0](LICENSE-APACHE) o [MIT License](LICENSE-MIT) a su elección.
