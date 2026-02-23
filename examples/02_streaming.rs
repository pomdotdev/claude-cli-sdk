//! Streaming query with model selection.
//!
//! Streams response tokens as they arrive instead of waiting for the full
//! response. Also demonstrates choosing a specific model.
//!
//! ```sh
//! cargo run --example 02_streaming
//! ```

use claude_cli_sdk::{ClientConfig, query_stream};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let config = ClientConfig::builder()
        .prompt("Explain ownership in Rust in three bullet points")
        .model("claude-sonnet-4-5")
        .build();

    let stream = query_stream(config).await?;
    tokio::pin!(stream);

    while let Some(msg) = stream.next().await {
        let msg = msg?;
        if let Some(text) = msg.assistant_text() {
            print!("{text}");
        }
    }
    println!();

    Ok(())
}
