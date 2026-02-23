//! Streaming query — yield messages as they arrive from Claude.
//!
//! ```sh
//! cargo run --example streaming -p claude-cli-sdk
//! ```

use claude_cli_sdk::{ClientConfig, Message, query_stream};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let config = ClientConfig::builder()
        .prompt("Explain async/await in Rust in 3 paragraphs")
        .model("claude-sonnet-4-5")
        .build();

    let stream = query_stream(config).await?;
    tokio::pin!(stream);

    while let Some(msg) = stream.next().await {
        let msg = msg?;
        match &msg {
            Message::Assistant(asst) => {
                if let Some(text) = msg.assistant_text() {
                    print!("{text}");
                }
                if let Some(reason) = &asst.message.stop_reason {
                    eprintln!("\n[stop_reason: {reason}]");
                }
            }
            Message::Result(result) => {
                if let Some(cost) = result.cost_usd {
                    eprintln!("[cost: ${cost:.4}]");
                }
            }
            _ => {}
        }
    }

    Ok(())
}
