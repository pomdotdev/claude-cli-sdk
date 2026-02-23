//! One-shot query — send a prompt and collect all response messages.
//!
//! ```sh
//! cargo run --example simple_query -p claude-cli-sdk
//! ```

use claude_cli_sdk::{ClientConfig, query};

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let config = ClientConfig::builder()
        .prompt("What is the Rust borrow checker?")
        .build();

    let messages = query(config).await?;

    for msg in &messages {
        if let Some(text) = msg.assistant_text() {
            println!("{text}");
        }
    }

    Ok(())
}
