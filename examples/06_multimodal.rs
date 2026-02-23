//! Multimodal input (image + text).
//!
//! Sends a URL-referenced image together with a text prompt using
//! `query_with_content`.
//!
//! ```sh
//! cargo run --example 06_multimodal
//! ```

use claude_cli_sdk::{ClientConfig, UserContent, query_with_content};

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let config = ClientConfig::builder()
        .prompt("placeholder") // ignored when content blocks are provided
        .build();

    let content = vec![
        UserContent::image_url(
            "https://upload.wikimedia.org/wikipedia/commons/thumb/d/d5/Rust_programming_language_black_logo.svg/240px-Rust_programming_language_black_logo.svg.png",
            "image/png",
        )?,
        UserContent::text("What logo is this? Answer in one sentence."),
    ];

    let messages = query_with_content(content, config).await?;

    for msg in &messages {
        if let Some(text) = msg.assistant_text() {
            println!("{text}");
        }
    }

    Ok(())
}
