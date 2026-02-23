//! Multi-turn conversation using the stateful `Client`.
//!
//! ```sh
//! cargo run --example multi_turn -p claude-cli-sdk
//! ```

use claude_cli_sdk::{Client, ClientConfig, Message};
use tokio_stream::StreamExt;

/// Drain a turn's messages and print assistant text.
async fn drain_turn(stream: impl tokio_stream::Stream<Item = claude_cli_sdk::Result<Message>>) {
    tokio::pin!(stream);
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(msg) => {
                if let Some(text) = msg.assistant_text() {
                    print!("{text}");
                }
                if matches!(&msg, Message::Result(_)) {
                    println!();
                    break;
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
                break;
            }
        }
    }
}

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let config = ClientConfig::builder()
        .prompt("Hello! Remember the number 42.")
        .build();

    let mut client = Client::new(config)?;
    let info = client.connect().await?;
    eprintln!("[session: {}]", info.session_id);

    // Turn 1: initial prompt (sent via config).
    eprintln!("\n--- Turn 1 ---");
    let stream = client.send("Hello! Remember the number 42.")?;
    drain_turn(stream).await;

    // Turn 2: follow-up question.
    eprintln!("--- Turn 2 ---");
    let stream = client.send("What number did I ask you to remember?")?;
    drain_turn(stream).await;

    client.close().await?;
    Ok(())
}
