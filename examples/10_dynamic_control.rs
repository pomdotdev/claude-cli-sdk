//! Dynamic control: runtime model switch + MCP servers.
//!
//! Opens a multi-turn session, switches the model mid-session using
//! `Client::set_model()`, and attaches an MCP server for external tool access.
//!
//! ```sh
//! cargo run --example 10_dynamic_control
//! ```

use std::time::Duration;

use claude_cli_sdk::{Client, ClientConfig, McpServerConfig, McpServers, PermissionMode};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    // Configure an MCP filesystem server.
    let mut servers = McpServers::new();
    servers.insert(
        "filesystem".into(),
        McpServerConfig::new("npx").with_args([
            "-y",
            "@modelcontextprotocol/server-filesystem",
            "/tmp",
        ]),
    );

    let config = ClientConfig::builder()
        .prompt("What model are you?")
        .model("claude-sonnet-4-5")
        .permission_mode(PermissionMode::AcceptEdits)
        .mcp_servers(servers)
        .connect_timeout(Some(Duration::from_secs(60)))
        .build();

    let mut client = Client::new(config)?;
    let info = client.connect().await?;
    println!("Session: {}", info.session_id);

    // Turn 1 — uses the initial model.
    {
        let stream = client.receive_messages()?;
        tokio::pin!(stream);
        while let Some(msg) = stream.next().await {
            let msg = msg?;
            if let Some(text) = msg.assistant_text() {
                println!("[turn 1] {text}");
            }
            if matches!(msg, claude_cli_sdk::Message::Result(_)) {
                break;
            }
        }
    }

    // Switch model mid-session.
    println!("\n--- switching model to claude-haiku-4-5 ---\n");
    client.set_model(Some("claude-haiku-4-5")).await?;

    // Turn 2 — uses the new model.
    {
        let stream2 = client.send("What model are you now?")?;
        tokio::pin!(stream2);
        while let Some(msg) = stream2.next().await {
            let msg = msg?;
            if let Some(text) = msg.assistant_text() {
                println!("[turn 2] {text}");
            }
        }
    }

    client.close().await?;
    Ok(())
}
