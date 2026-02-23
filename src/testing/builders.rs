//! Convenience builders for constructing test messages.

use crate::types::content::{ContentBlock, TextBlock};
use crate::types::messages::*;

/// Build a `Message::System` for testing (simulated init message).
#[must_use]
pub fn system_init(session_id: &str) -> Message {
    Message::System(SystemMessage {
        subtype: "init".into(),
        session_id: session_id.into(),
        cwd: "/tmp".into(),
        tools: vec!["bash".into(), "read_file".into(), "write_file".into()],
        mcp_servers: vec![],
        model: "claude-sonnet-4-5".into(),
        extra: serde_json::Value::Object(Default::default()),
    })
}

/// Build a `Message::System` with custom tools.
#[must_use]
pub fn system_init_with_tools(session_id: &str, tools: Vec<String>) -> Message {
    Message::System(SystemMessage {
        subtype: "init".into(),
        session_id: session_id.into(),
        cwd: "/tmp".into(),
        tools,
        mcp_servers: vec![],
        model: "claude-sonnet-4-5".into(),
        extra: serde_json::Value::Object(Default::default()),
    })
}

/// Build a `Message::Assistant` with text content.
#[must_use]
pub fn assistant_text(text: &str) -> Message {
    Message::Assistant(AssistantMessage {
        message: AssistantMessageInner {
            id: format!("msg_{}", uuid::Uuid::new_v4()),
            content: vec![ContentBlock::Text(TextBlock { text: text.into() })],
            model: "claude-sonnet-4-5".into(),
            stop_reason: Some("end_turn".into()),
            usage: Usage {
                input_tokens: 100,
                output_tokens: 50,
                ..Default::default()
            },
        },
        session_id: None,
        extra: serde_json::Value::Object(Default::default()),
    })
}

/// Build a `Message::Assistant` with text content and a specific session ID.
#[must_use]
pub fn assistant_text_with_session(text: &str, session_id: &str) -> Message {
    let mut msg = assistant_text(text);
    if let Message::Assistant(ref mut a) = msg {
        a.session_id = Some(session_id.into());
    }
    msg
}

/// Build a successful `Message::Result`.
#[must_use]
pub fn result_success(session_id: &str) -> Message {
    Message::Result(ResultMessage {
        subtype: "success".into(),
        cost_usd: Some(0.001),
        duration_ms: 1000,
        duration_api_ms: 900,
        is_error: false,
        num_turns: 1,
        session_id: Some(session_id.into()),
        total_cost_usd: Some(0.001),
        usage: Usage {
            input_tokens: 100,
            output_tokens: 50,
            ..Default::default()
        },
        result: Some("Done.".into()),
        extra: serde_json::Value::Object(Default::default()),
    })
}

/// Build an error `Message::Result`.
#[must_use]
pub fn result_error(session_id: &str) -> Message {
    Message::Result(ResultMessage {
        subtype: "error".into(),
        cost_usd: None,
        duration_ms: 500,
        duration_api_ms: 0,
        is_error: true,
        num_turns: 0,
        session_id: Some(session_id.into()),
        total_cost_usd: None,
        usage: Usage::default(),
        result: None,
        extra: serde_json::Value::Object(Default::default()),
    })
}

/// Build a `Message::User` with text content.
#[must_use]
pub fn user_text(text: &str) -> Message {
    Message::User(UserMessage {
        message: UserMessageInner {
            role: "user".into(),
            content: serde_json::json!(text),
        },
        extra: serde_json::Value::Object(Default::default()),
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_init_creates_valid_message() {
        let msg = system_init("s1");
        assert_eq!(msg.session_id(), Some("s1"));
        if let Message::System(s) = &msg {
            assert_eq!(s.subtype, "init");
        } else {
            panic!("expected System");
        }
    }

    #[test]
    fn assistant_text_creates_valid_message() {
        let msg = assistant_text("Hello!");
        assert_eq!(msg.assistant_text(), Some("Hello!".into()));
    }

    #[test]
    fn result_success_is_not_error() {
        let msg = result_success("s1");
        assert!(!msg.is_error_result());
    }

    #[test]
    fn result_error_is_error() {
        let msg = result_error("s1");
        assert!(msg.is_error_result());
    }

    #[test]
    fn user_text_creates_user_message() {
        let msg = user_text("hi");
        assert!(matches!(msg, Message::User(_)));
    }

    #[test]
    fn all_builders_serialize() {
        let messages = [
            system_init("s1"),
            assistant_text("test"),
            result_success("s1"),
            result_error("s1"),
            user_text("hello"),
        ];
        for msg in &messages {
            let json = serde_json::to_string(msg);
            assert!(json.is_ok(), "failed to serialize: {msg:?}");
        }
    }
}
