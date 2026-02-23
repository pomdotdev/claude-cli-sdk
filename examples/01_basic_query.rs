//! Basic one-shot query.
//!
//! Sends a single prompt to Claude and prints the response.
//!
//! ```sh
//! cargo run --example 01_basic_query
//! ```

use claude_cli_sdk::{ClientConfig, query};

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let config = ClientConfig::builder()
        .prompt("What is the Rust programming language in one sentence?")
        .build();

    let messages = query(config).await?;

    for msg in &messages {
        if let Some(text) = msg.assistant_text() {
            println!("{text}");
        }
    }

    Ok(())
}
