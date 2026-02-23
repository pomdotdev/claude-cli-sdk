//! Extended thinking + fallback model.
//!
//! Enables extended thinking tokens and sets a fallback model in case the
//! primary is unavailable. Prints both thinking and response text.
//!
//! ```sh
//! cargo run --example 07_thinking_and_fallback
//! ```

use claude_cli_sdk::{ClientConfig, ContentBlock, Message, query};

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let config = ClientConfig::builder()
        .prompt("What is the sum of the first 100 prime numbers? Think step by step.")
        .model("claude-sonnet-4-5")
        .fallback_model("claude-haiku-4-5")
        .max_thinking_tokens(10_000_u32)
        .build();

    let messages = query(config).await?;

    for msg in &messages {
        if let Message::Assistant(assistant) = msg {
            for block in &assistant.message.content {
                match block {
                    ContentBlock::Thinking(t) => {
                        println!("--- thinking ---");
                        // Thinking text can be long; show a preview.
                        let preview: String = t.thinking.chars().take(200).collect();
                        println!("{preview}...");
                        println!("--- end thinking ---\n");
                    }
                    ContentBlock::Text(t) => {
                        println!("{}", t.text);
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
