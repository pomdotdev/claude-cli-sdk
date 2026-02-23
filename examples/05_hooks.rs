//! Lifecycle hooks.
//!
//! Registers hooks for three events: `PreToolUse` (log & optionally block),
//! `PostToolUse` (observe results), and `Stop` (session teardown).
//!
//! ```sh
//! cargo run --example 05_hooks
//! ```

use std::sync::Arc;

use claude_cli_sdk::{ClientConfig, HookEvent, HookMatcher, HookOutput, PermissionMode, query};

#[tokio::main]
async fn main() -> claude_cli_sdk::Result<()> {
    let config = ClientConfig::builder()
        .prompt("List files in the current directory")
        .permission_mode(PermissionMode::AcceptEdits)
        .hooks(vec![
            // Log every tool invocation before it runs.
            HookMatcher::new(
                HookEvent::PreToolUse,
                Arc::new(|input, _session, _ctx| {
                    Box::pin(async move {
                        eprintln!(
                            "[hook:pre]  tool={:?} input={:?}",
                            input.tool_name, input.tool_input
                        );
                        HookOutput::allow()
                    })
                }),
            ),
            // Log Bash results after execution.
            HookMatcher::new(
                HookEvent::PostToolUse,
                Arc::new(|input, _session, _ctx| {
                    Box::pin(async move {
                        eprintln!(
                            "[hook:post] tool={:?} result={:?}",
                            input.tool_name, input.tool_result
                        );
                        HookOutput::allow()
                    })
                }),
            )
            .for_tool("Bash"),
            // Log when the session stops.
            HookMatcher::new(
                HookEvent::Stop,
                Arc::new(|_input, _session, _ctx| {
                    Box::pin(async move {
                        eprintln!("[hook:stop] session ended");
                        HookOutput::allow()
                    })
                }),
            ),
        ])
        .build();

    let messages = query(config).await?;

    for msg in &messages {
        if let Some(text) = msg.assistant_text() {
            println!("{text}");
        }
    }

    Ok(())
}
