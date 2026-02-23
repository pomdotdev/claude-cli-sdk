//! Multi-turn session.
//!
//! Opens a persistent session and sends multiple prompts, demonstrating
//! the `Client::send()` / `receive_messages()` pattern.
//!
//! ```sh
//! cargo run --example 03_multi_turn
//! ```

use claude_cli_sdk::{Client, ClientConfig};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let config = ClientConfig::builder().prompt("What is 2 + 2?").build();

    let mut client = Client::new(config)?;
    let session_info = client.connect().await?;
    println!("Session: {}", session_info.session_id);

    // First turn — the prompt from config is sent automatically.
    {
        let stream = client.receive_messages()?;
        tokio::pin!(stream);
        while let Some(msg) = stream.next().await {
            let msg = msg?;
            if let Some(text) = msg.assistant_text() {
                println!("Turn 1: {text}");
            }
            if matches!(msg, claude_cli_sdk::Message::Result(_)) {
                break;
            }
        }
    }

    // Second turn — send a follow-up.
    {
        let stream2 = client.send("Now multiply that by 10")?;
        tokio::pin!(stream2);
        while let Some(msg) = stream2.next().await {
            let msg = msg?;
            if let Some(text) = msg.assistant_text() {
                println!("Turn 2: {text}");
            }
        }
    }

    client.close().await?;
    Ok(())
}
