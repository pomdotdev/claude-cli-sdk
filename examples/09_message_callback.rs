//! Message callback + stderr debugging.
//!
//! Registers a `MessageCallback` to observe every message and a
//! `stderr_callback` to capture CLI debug output.
//!
//! ```sh
//! cargo run --example 09_message_callback
//! ```

use std::sync::Arc;

use claude_cli_sdk::{ClientConfig, Message, MessageCallback, query};

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    // Observe every message passing through the SDK.
    let callback: MessageCallback = Arc::new(|msg: Message| {
        match &msg {
            Message::System(sys) => {
                eprintln!("[callback] system init, model={}", sys.model);
            }
            Message::Assistant(a) => {
                eprintln!(
                    "[callback] assistant message, {} content block(s)",
                    a.message.content.len()
                );
            }
            Message::Result(r) => {
                eprintln!("[callback] result, is_error={}", r.is_error);
            }
            _ => {
                eprintln!("[callback] other message type");
            }
        }
        Some(msg) // pass through unchanged
    });

    let config = ClientConfig::builder()
        .prompt("Say hello in three languages")
        .message_callback(callback)
        .verbose(true)
        .stderr_callback(Arc::new(|line: String| {
            eprintln!("[stderr] {line}");
        }))
        .build();

    let messages = query(config).await?;

    for msg in &messages {
        if let Some(text) = msg.assistant_text() {
            println!("{text}");
        }
    }

    Ok(())
}
