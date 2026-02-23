//! Message callback — a simple observe/filter hook for SDK consumers.
//!
//! The [`MessageCallback`] is invoked for each [`Message`]
//! received from the CLI. It can:
//!
//! - **Observe** messages (return `Some(msg)` unchanged)
//! - **Transform** messages (return `Some(modified_msg)`)
//! - **Filter** messages (return `None` to suppress)
//!
//! This is a lightweight SDK-level hook for message observation and filtering.
//!
//! # Example
//!
//! ```rust
//! use std::sync::Arc;
//! use claude_cli_sdk::callback::MessageCallback;
//! use claude_cli_sdk::Message;
//!
//! // Log all messages, pass them through unchanged:
//! let logger: MessageCallback = Arc::new(|msg: Message| {
//!     eprintln!("received: {msg:?}");
//!     Some(msg)
//! });
//!
//! // Filter out system messages:
//! let filter: MessageCallback = Arc::new(|msg: Message| {
//!     match &msg {
//!         Message::System(_) => None,
//!         _ => Some(msg),
//!     }
//! });
//! ```

use std::sync::Arc;

use crate::types::messages::Message;

/// Optional callback invoked for each message received from the CLI.
///
/// - Return `Some(msg)` to pass the message through (possibly transformed).
/// - Return `None` to filter the message out of the stream.
///
/// When no callback is configured, all messages pass through unchanged.
pub type MessageCallback = Arc<dyn Fn(Message) -> Option<Message> + Send + Sync>;

/// Apply a message callback to a message, or pass through if no callback is set.
#[inline]
pub fn apply_callback(msg: Message, callback: Option<&MessageCallback>) -> Option<Message> {
    match callback {
        Some(cb) => cb(msg),
        None => Some(msg),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::content::TextBlock;
    use crate::types::messages::*;

    fn make_system_msg() -> Message {
        Message::System(SystemMessage {
            subtype: "init".into(),
            session_id: "s1".into(),
            cwd: "/tmp".into(),
            tools: vec![],
            mcp_servers: vec![],
            model: "test".into(),
            extra: serde_json::Value::Object(Default::default()),
        })
    }

    fn make_assistant_msg(text: &str) -> Message {
        Message::Assistant(AssistantMessage {
            message: AssistantMessageInner {
                id: "m1".into(),
                content: vec![crate::types::content::ContentBlock::Text(TextBlock {
                    text: text.into(),
                })],
                model: "test".into(),
                stop_reason: None,
                usage: Usage::default(),
            },
            session_id: None,
            extra: serde_json::Value::Object(Default::default()),
        })
    }

    #[test]
    fn no_callback_passes_through() {
        let msg = make_system_msg();
        let result = apply_callback(msg.clone(), None);
        assert!(result.is_some());
    }

    #[test]
    fn callback_can_filter() {
        let filter: MessageCallback = Arc::new(|msg| match &msg {
            Message::System(_) => None,
            _ => Some(msg),
        });

        assert!(apply_callback(make_system_msg(), Some(&filter)).is_none());
        assert!(apply_callback(make_assistant_msg("hi"), Some(&filter)).is_some());
    }

    #[test]
    fn callback_can_transform() {
        let transform: MessageCallback = Arc::new(|msg| {
            // Pass through but we could modify here
            Some(msg)
        });
        let msg = make_assistant_msg("hello");
        let result = apply_callback(msg, Some(&transform));
        assert!(result.is_some());
    }

    #[test]
    fn callback_can_observe() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = Arc::clone(&count);

        let observer: MessageCallback = Arc::new(move |msg| {
            count_clone.fetch_add(1, Ordering::Relaxed);
            Some(msg)
        });

        apply_callback(make_system_msg(), Some(&observer));
        apply_callback(make_assistant_msg("test"), Some(&observer));

        assert_eq!(count.load(Ordering::Relaxed), 2);
    }
}
