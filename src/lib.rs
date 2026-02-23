#![warn(missing_docs)]
//! # claude-cli-sdk
//!
//! Rust SDK for programmatic interaction with the [Claude Code CLI].
//!
//! This crate provides strongly-typed, async-first access to Claude Code
//! sessions via the CLI's NDJSON stdio protocol.
//!
//! ## Platform support
//!
//! This SDK supports macOS, Linux, and Windows.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use claude_cli_sdk::{query, ClientConfig};
//!
//! #[tokio::main]
//! async fn main() -> claude_cli_sdk::Result<()> {
//!     let config = ClientConfig::builder()
//!         .prompt("List files in /tmp")
//!         .build();
//!     let messages = query(config).await?;
//!     for msg in &messages {
//!         if let Some(text) = msg.assistant_text() {
//!             println!("{text}");
//!         }
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Timeouts
//!
//! All timeouts are configurable via [`ClientConfig`]:
//!
//! | Timeout | Default | Purpose |
//! |---------|---------|---------|
//! | `connect_timeout` | 30s | Deadline for process spawn + init message |
//! | `close_timeout` | 10s | Deadline for graceful shutdown; kills on expiry |
//! | `read_timeout` | None | Per-message recv deadline (detects hung processes) |
//! | `default_hook_timeout` | 30s | Fallback when `HookMatcher::timeout` is None |
//! | `version_check_timeout` | 5s | Deadline for `--version` check |
//!
//! Set any `Option<Duration>` to `None` to wait indefinitely.
//!
//! ## Feature flags
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `testing` | Enables `testing` utilities (e.g., `MockTransport`) |
//! | `efficiency` | Reserved for future throughput optimisations |
//! | `integration` | Enables integration-test helpers (requires a live CLI) |
//!
//! [Claude Code CLI]: https://www.anthropic.com/claude-code

pub mod callback;
pub mod client;
pub mod config;
pub mod discovery;
pub mod errors;
pub mod hooks;
pub mod mcp;
pub mod permissions;
pub mod transport;
pub mod types;
pub(crate) mod util;

#[cfg(feature = "testing")]
pub mod testing;

// ── Top-level re-exports ────────────────────────────────────────────────────

// Core
pub use client::Client;
pub use config::{ClientConfig, PermissionMode, SystemPrompt};
pub use errors::{Error, Result};

// Content types
pub use types::content::{
    ALLOWED_IMAGE_MIME_TYPES, Base64ImageSource, ContentBlock, ImageBlock, ImageSource,
    MAX_IMAGE_BASE64_BYTES, TextBlock, ThinkingBlock, ToolResultBlock, ToolResultContent,
    ToolUseBlock, UrlImageSource, UserContent,
};

// Message types
pub use types::messages::{
    AssistantMessage, AssistantMessageInner, McpServerStatus, Message, ResultMessage, SessionInfo,
    StreamEvent, SystemMessage, Usage, UserMessage, UserMessageInner,
};

// Permissions
pub use permissions::{CanUseToolCallback, PermissionContext, PermissionDecision};

// Hooks
pub use hooks::{
    HookCallback, HookContext, HookDecision, HookEvent, HookInput, HookMatcher, HookOutput,
};

// MCP
pub use mcp::{McpServerConfig, McpServers};

// Discovery
pub use discovery::{check_cli_version, find_cli};

// Transport
pub use transport::Transport;

// Cancellation
pub use tokio_util::sync::CancellationToken;

// Callback
pub use callback::MessageCallback;

use futures_core::Stream;

// ── Shared stream helper ────────────────────────────────────────────────────

/// Collect all messages from a stream into a vector.
async fn collect_stream(stream: impl Stream<Item = Result<Message>>) -> Result<Vec<Message>> {
    use tokio_stream::StreamExt;
    tokio::pin!(stream);
    let mut messages = Vec::new();
    while let Some(msg) = stream.next().await {
        messages.push(msg?);
    }
    Ok(messages)
}

// ── Top-level free functions ────────────────────────────────────────────────

/// Run a one-shot query against Claude Code and collect all response messages.
///
/// This is the simplest way to use the SDK. It spawns a CLI process, sends the
/// prompt from `config`, collects all response messages, and shuts down.
///
/// # Example
///
/// ```rust,no_run
/// use claude_cli_sdk::{query, ClientConfig};
///
/// # async fn example() -> claude_cli_sdk::Result<()> {
/// let config = ClientConfig::builder()
///     .prompt("What is Rust?")
///     .build();
/// let messages = query(config).await?;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns [`Error`] if the CLI cannot be found, spawning fails, or the
/// session encounters an error.
#[must_use = "the future must be awaited to run the query"]
pub async fn query(config: ClientConfig) -> Result<Vec<Message>> {
    let stream = query_stream(config).await?;
    collect_stream(stream).await
}

/// Run a streaming query against Claude Code, yielding messages as they arrive.
///
/// Returns a `Stream` of [`Message`] values. The stream ends when the CLI
/// emits a `Result` message.
///
/// # Example
///
/// ```rust,no_run
/// use claude_cli_sdk::{query_stream, ClientConfig, Message};
/// use tokio_stream::StreamExt;
///
/// # async fn example() -> claude_cli_sdk::Result<()> {
/// let config = ClientConfig::builder()
///     .prompt("Explain async/await in Rust")
///     .build();
/// let mut stream = query_stream(config).await?;
/// tokio::pin!(stream);
///
/// while let Some(msg) = stream.next().await {
///     let msg = msg?;
///     if let Some(text) = msg.assistant_text() {
///         print!("{text}");
///     }
/// }
/// # Ok(())
/// # }
/// ```
#[must_use = "the future must be awaited to run the query"]
pub async fn query_stream(config: ClientConfig) -> Result<impl Stream<Item = Result<Message>>> {
    let cancel = config.cancellation_token.clone();
    let mut client = Client::new(config)?;
    client.connect().await?;

    let read_timeout = client.read_timeout();

    // Take the message receiver and own it in the stream.
    // The client stays alive because the transport/background task are Arc'd.
    let rx = client.take_message_rx().ok_or(Error::NotConnected)?;

    Ok(async_stream::stream! {
        loop {
            match crate::client::recv_with_timeout(&rx, read_timeout, cancel.as_ref()).await {
                Ok(msg) => yield Ok(msg),
                Err(ref e) if matches!(e, crate::errors::Error::Transport(_)) => break,
                Err(e) => {
                    yield Err(e);
                    break;
                }
            }
        }
        // Explicitly close the client to clean up the CLI process.
        let _ = client.close().await;
    })
}

/// Run a one-shot multi-modal query and collect all response messages.
///
/// Like [`query()`] but accepts structured content blocks (text + images)
/// instead of a plain string prompt. The `config.prompt` field is ignored
/// when content blocks are provided.
///
/// # Errors
///
/// Returns [`Error::Config`] if `content` is empty.
#[must_use = "the future must be awaited to run the query"]
pub async fn query_with_content(
    content: Vec<UserContent>,
    config: ClientConfig,
) -> Result<Vec<Message>> {
    let stream = query_stream_with_content(content, config).await?;
    collect_stream(stream).await
}

/// Run a streaming multi-modal query, yielding messages as they arrive.
///
/// Like [`query_stream()`] but accepts structured content blocks.
/// The `config.prompt` field is ignored — content blocks are sent via stdin.
///
/// # Errors
///
/// Returns [`Error::Config`] if `content` is empty.
#[must_use = "the future must be awaited to run the query"]
pub async fn query_stream_with_content(
    content: Vec<UserContent>,
    config: ClientConfig,
) -> Result<impl Stream<Item = Result<Message>>> {
    if content.is_empty() {
        return Err(Error::Config("content must not be empty".into()));
    }

    let cancel = config.cancellation_token.clone();
    let mut client = Client::new(config)?;
    client.connect().await?;

    let read_timeout = client.read_timeout();

    // Send the content blocks as a structured user message.
    let user_message = serde_json::json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": content
        }
    });
    let json = serde_json::to_string(&user_message).map_err(Error::Json)?;
    client.transport_write(&json).await?;

    let rx = client.take_message_rx().ok_or(Error::NotConnected)?;

    Ok(async_stream::stream! {
        loop {
            match crate::client::recv_with_timeout(&rx, read_timeout, cancel.as_ref()).await {
                Ok(msg) => yield Ok(msg),
                Err(ref e) if matches!(e, crate::errors::Error::Transport(_)) => break,
                Err(e) => {
                    yield Err(e);
                    break;
                }
            }
        }
        let _ = client.close().await;
    })
}
