//! Permission callbacks.
//!
//! Demonstrates programmatic tool approval using `CanUseToolCallback`.
//! Only file-read tools are allowed; everything else is denied.
//!
//! ```sh
//! cargo run --example 04_permissions
//! ```

use std::sync::Arc;

use claude_cli_sdk::{ClientConfig, PermissionContext, PermissionDecision, query};

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let config = ClientConfig::builder()
        .prompt("Read the file Cargo.toml and summarise it")
        .can_use_tool(Arc::new(
            |tool_name: &str, _input: &serde_json::Value, _ctx: PermissionContext| {
                let tool_name = tool_name.to_owned();
                Box::pin(async move {
                    if tool_name.starts_with("Read") {
                        eprintln!("[permission] ALLOW {tool_name}");
                        PermissionDecision::allow()
                    } else {
                        eprintln!("[permission] DENY  {tool_name}");
                        PermissionDecision::deny(format!("{tool_name} is not allowed"))
                    }
                })
            },
        ))
        .build();

    let messages = query(config).await?;

    for msg in &messages {
        if let Some(text) = msg.assistant_text() {
            println!("{text}");
        }
    }

    Ok(())
}
