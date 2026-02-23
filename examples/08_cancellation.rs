//! Cooperative cancellation.
//!
//! Starts a streaming query and cancels it after 3 seconds using a
//! `CancellationToken`. Demonstrates graceful early termination.
//!
//! ```sh
//! cargo run --example 08_cancellation
//! ```

use std::time::Duration;

use claude_cli_sdk::{CancellationToken, ClientConfig, query_stream};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let token = CancellationToken::new();

    let config = ClientConfig::builder()
        .prompt("Write a very long essay about the history of computing")
        .cancellation_token(token.clone())
        .build();

    // Cancel after 3 seconds in a background task.
    let cancel_token = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        eprintln!("[cancellation] firing token after 3s");
        cancel_token.cancel();
    });

    let stream = query_stream(config).await?;
    tokio::pin!(stream);

    while let Some(result) = stream.next().await {
        match result {
            Ok(msg) => {
                if let Some(text) = msg.assistant_text() {
                    print!("{text}");
                }
            }
            Err(e) if e.is_cancelled() => {
                eprintln!("\n[cancelled] operation was cancelled gracefully");
                break;
            }
            Err(e) => return Err(e),
        }
    }

    Ok(())
}
