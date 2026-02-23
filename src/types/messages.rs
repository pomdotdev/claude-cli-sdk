//! Top-level NDJSON message types produced by the Claude Code CLI.
//!
//! The CLI emits newline-delimited JSON (NDJSON) on its stdout. Each line is
//! one [`Message`]. This module defines the full type hierarchy for all known
//! message variants.
//!
//! # Resilience
//!
//! All struct fields use `#[serde(default)]` where appropriate so that:
//! - Missing fields (older CLI versions) do not cause parse failures.
//! - New fields (future CLI versions) are silently ignored unless captured by
//!   the `#[serde(flatten)] extra` pattern.
//!
//! # Example
//!
//! ```rust
//! use claude_cli_sdk::types::messages::Message;
//!
//! let line = r#"{"type":"system","subtype":"init","session_id":"s1","cwd":"/tmp","tools":[],"mcp_servers":[],"model":"claude-opus-4-5"}"#;
//! let msg: Message = serde_json::from_str(line).unwrap();
//! match msg {
//!     Message::System(s) => println!("session: {}", s.session_id),
//!     _ => {}
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::types::content::ContentBlock;

// ── Top-level message enum ────────────────────────────────────────────────────

/// The top-level message type emitted by the Claude Code CLI on stdout.
///
/// Each NDJSON line deserialises into one of these variants.  The `"type"`
/// field drives the outer discriminant; inner subtypes are carried by nested
/// fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    /// A system-level message (e.g., session initialisation, tool list).
    System(SystemMessage),
    /// A message produced by the assistant (Claude).
    Assistant(AssistantMessage),
    /// A user turn in the conversation.
    User(UserMessage),
    /// The final result message containing cost and usage summary.
    Result(ResultMessage),
    /// A streaming event emitted during tool execution or other async operations.
    #[serde(rename = "stream_event")]
    StreamEvent(StreamEvent),
    /// A rate limit status event emitted after API calls.
    #[serde(rename = "rate_limit_event")]
    RateLimitEvent(RateLimitEvent),
}

impl Message {
    /// Returns the session ID if present on this message variant.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::System(m) => Some(&m.session_id),
            Self::Assistant(m) => m.session_id.as_deref(),
            Self::User(_) => None,
            Self::Result(m) => m.session_id.as_deref(),
            Self::StreamEvent(m) => Some(&m.session_id),
            Self::RateLimitEvent(m) => m.session_id.as_deref(),
        }
    }

    /// Returns `true` if this is a [`Message::Result`] that indicates an error.
    #[inline]
    #[must_use]
    pub fn is_error_result(&self) -> bool {
        matches!(self, Self::Result(r) if r.is_error)
    }

    /// Returns `true` if this is a [`Message::StreamEvent`].
    #[inline]
    #[must_use]
    pub fn is_stream_event(&self) -> bool {
        matches!(self, Self::StreamEvent(_))
    }

    /// Extract text content from an [`Message::Assistant`] message, joining
    /// all [`ContentBlock::Text`] blocks with newlines.
    #[must_use]
    pub fn assistant_text(&self) -> Option<String> {
        if let Self::Assistant(a) = self {
            let text: String = a
                .message
                .content
                .iter()
                .filter_map(|b| b.as_text())
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() { None } else { Some(text) }
        } else {
            None
        }
    }
}

// ── SystemMessage ─────────────────────────────────────────────────────────────

/// A system-level message.  The first message emitted by the CLI is always a
/// `system/init` message that carries session bootstrap information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemMessage {
    /// Discriminates between system message kinds (e.g., `"init"`).
    #[serde(default)]
    pub subtype: String,

    /// Opaque session identifier.
    #[serde(default)]
    pub session_id: String,

    /// Working directory of the Claude process.
    #[serde(default)]
    pub cwd: String,

    /// List of tool names available in this session.
    #[serde(default)]
    pub tools: Vec<String>,

    /// MCP server statuses reported at init.
    #[serde(default)]
    pub mcp_servers: Vec<McpServerStatus>,

    /// Model identifier (e.g., `"claude-opus-4-5"`).
    #[serde(default)]
    pub model: String,

    /// Forward-compatibility: absorbs any fields not explicitly listed above.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Status of an MCP (Model Context Protocol) server at session init.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServerStatus {
    /// Server name as configured.
    pub name: String,

    /// Connection status string (e.g., `"connected"`, `"failed"`).
    #[serde(default)]
    pub status: String,
}

// ── AssistantMessage ──────────────────────────────────────────────────────────

/// An assistant turn containing one or more content blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssistantMessage {
    /// The inner Anthropic messages API object.
    pub message: AssistantMessageInner,

    /// Session ID (may be absent on streaming deltas).
    #[serde(default)]
    pub session_id: Option<String>,

    /// Forward-compatibility catchall.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// The inner structure of an assistant message, mirroring the Anthropic
/// messages API response format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssistantMessageInner {
    /// Unique message ID assigned by the API.
    #[serde(default)]
    pub id: String,

    /// Content blocks (text, tool use, thinking, images, …).
    #[serde(default)]
    pub content: Vec<ContentBlock>,

    /// Model that generated this message.
    #[serde(default)]
    pub model: String,

    /// Why the model stopped generating (e.g., `"end_turn"`, `"tool_use"`).
    #[serde(default)]
    pub stop_reason: Option<String>,

    /// Token usage for this turn.
    #[serde(default)]
    pub usage: Usage,
}

// ── UserMessage ───────────────────────────────────────────────────────────────

/// A user-turn message (input submitted to Claude).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserMessage {
    /// The inner user message payload.
    pub message: UserMessageInner,

    /// Forward-compatibility catchall.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// The inner structure of a user message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserMessageInner {
    /// Role is always `"user"` but kept for API symmetry.
    #[serde(default)]
    pub role: String,

    /// The content of this user turn.  In the protocol this may be a plain
    /// string or a list of content blocks; we normalise to the list form.
    #[serde(default)]
    pub content: serde_json::Value,
}

// ── ResultMessage ─────────────────────────────────────────────────────────────

/// The final message emitted by the CLI after the session completes.
///
/// Contains cost accounting and usage aggregates for the entire session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultMessage {
    /// Discriminates result subtypes (e.g., `"success"`, `"error"`).
    #[serde(default)]
    pub subtype: String,

    /// USD cost of this session (may be absent if billing is unavailable).
    #[serde(default)]
    pub cost_usd: Option<f64>,

    /// Wall-clock duration of the session in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,

    /// API-only duration (excludes local processing) in milliseconds.
    #[serde(default)]
    pub duration_api_ms: u64,

    /// `true` if the session ended due to an error.
    #[serde(default)]
    pub is_error: bool,

    /// Number of conversation turns executed.
    #[serde(default)]
    pub num_turns: u32,

    /// Session identifier.
    #[serde(default)]
    pub session_id: Option<String>,

    /// Cumulative USD cost across all sessions (for resumed sessions).
    #[serde(default)]
    pub total_cost_usd: Option<f64>,

    /// Aggregate token usage for the session.
    #[serde(default)]
    pub usage: Usage,

    /// The final text result produced by the session, if any.
    #[serde(default)]
    pub result: Option<String>,

    /// Forward-compatibility catchall.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ── StreamEvent ──────────────────────────────────────────────────────────────

/// A streaming event emitted during tool execution or other async operations.
///
/// Stream events carry opaque event data and are associated with a session
/// and optionally with a parent tool use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamEvent {
    /// Unique event identifier.
    pub uuid: String,

    /// Session this event belongs to.
    pub session_id: String,

    /// Opaque event payload.
    pub event: serde_json::Value,

    /// The tool use ID this event is associated with (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,

    /// Forward-compatibility catchall.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ── Rate Limit ───────────────────────────────────────────────────────────────

/// A rate-limit event emitted by the CLI when the API returns a 429.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateLimitEvent {
    /// Session this event belongs to (if present).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Forward-compatibility catchall — captures all rate-limit fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ── Usage ─────────────────────────────────────────────────────────────────────

/// Token usage counters for a message or session.
///
/// All fields default to `0` so that partial responses (e.g., streaming
/// deltas) do not cause parse failures.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Tokens in the input (prompt) context.
    #[serde(default)]
    pub input_tokens: u32,

    /// Tokens in the generated output.
    #[serde(default)]
    pub output_tokens: u32,

    /// Input tokens served from the prompt cache.
    #[serde(default)]
    pub cache_read_input_tokens: u32,

    /// Input tokens used to populate the prompt cache.
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
}

impl Usage {
    /// Total tokens consumed (input + output), ignoring cache counters.
    #[inline]
    #[must_use]
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens.saturating_add(self.output_tokens)
    }

    /// Merge another `Usage` into `self` by summing all counters.
    #[inline]
    pub fn merge(&mut self, other: &Self) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cache_read_input_tokens = self
            .cache_read_input_tokens
            .saturating_add(other.cache_read_input_tokens);
        self.cache_creation_input_tokens = self
            .cache_creation_input_tokens
            .saturating_add(other.cache_creation_input_tokens);
    }
}

// ── SessionInfo ───────────────────────────────────────────────────────────────

/// Structured session bootstrap information, parsed from the first
/// [`SystemMessage`] with `subtype == "init"`.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionInfo {
    /// Opaque session identifier.
    pub session_id: String,
    /// Working directory of the Claude process.
    pub cwd: String,
    /// Available tool names.
    pub tools: Vec<String>,
    /// MCP server statuses.
    pub mcp_servers: Vec<McpServerStatus>,
    /// Model in use.
    pub model: String,
}

impl TryFrom<&SystemMessage> for SessionInfo {
    type Error = crate::errors::Error;

    fn try_from(msg: &SystemMessage) -> crate::errors::Result<Self> {
        if msg.subtype != "init" {
            return Err(crate::errors::Error::ControlProtocol(format!(
                "expected system/init message, got subtype '{}'",
                msg.subtype
            )));
        }
        Ok(Self {
            session_id: msg.session_id.clone(),
            cwd: msg.cwd.clone(),
            tools: msg.tools.clone(),
            mcp_servers: msg.mcp_servers.clone(),
            model: msg.model.clone(),
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::content::TextBlock;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn round_trip<T>(value: &T) -> T
    where
        T: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug + PartialEq,
    {
        let json = serde_json::to_string(value).expect("serialize");
        serde_json::from_str(&json).expect("deserialize")
    }

    // ── SystemMessage ─────────────────────────────────────────────────────────

    #[test]
    fn system_message_round_trip() {
        let msg = Message::System(SystemMessage {
            subtype: "init".into(),
            session_id: "sess-1".into(),
            cwd: "/home/user".into(),
            tools: vec!["bash".into(), "read_file".into()],
            mcp_servers: vec![McpServerStatus {
                name: "my-server".into(),
                status: "connected".into(),
            }],
            model: "claude-opus-4-5".into(),
            extra: serde_json::Value::Object(Default::default()),
        });
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn system_message_from_ndjson() {
        let line = r#"{"type":"system","subtype":"init","session_id":"s1","cwd":"/tmp","tools":["bash"],"mcp_servers":[],"model":"claude-opus-4-5"}"#;
        let msg: Message = serde_json::from_str(line).unwrap();
        match msg {
            Message::System(s) => {
                assert_eq!(s.subtype, "init");
                assert_eq!(s.session_id, "s1");
                assert_eq!(s.cwd, "/tmp");
                assert_eq!(s.tools, ["bash"]);
                assert_eq!(s.model, "claude-opus-4-5");
            }
            other => panic!("expected System, got {other:?}"),
        }
    }

    #[test]
    fn system_message_missing_fields_use_defaults() {
        // Only "type" and "subtype" — all other fields should default.
        let line = r#"{"type":"system","subtype":"status"}"#;
        let msg: Message = serde_json::from_str(line).unwrap();
        if let Message::System(s) = msg {
            assert_eq!(s.session_id, "");
            assert!(s.tools.is_empty());
        } else {
            panic!("expected System");
        }
    }

    #[test]
    fn system_message_extra_fields_preserved() {
        let line = r#"{"type":"system","subtype":"init","session_id":"s1","cwd":"/","tools":[],"mcp_servers":[],"model":"m","future_field":"value"}"#;
        let msg: Message = serde_json::from_str(line).unwrap();
        if let Message::System(s) = msg {
            assert_eq!(s.extra["future_field"], "value");
        }
    }

    // ── AssistantMessage ──────────────────────────────────────────────────────

    #[test]
    fn assistant_message_round_trip() {
        let msg = Message::Assistant(AssistantMessage {
            message: AssistantMessageInner {
                id: "msg_001".into(),
                content: vec![ContentBlock::Text(TextBlock {
                    text: "Hello!".into(),
                })],
                model: "claude-opus-4-5".into(),
                stop_reason: Some("end_turn".into()),
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                    cache_read_input_tokens: 0,
                    cache_creation_input_tokens: 0,
                },
            },
            session_id: Some("sess-1".into()),
            extra: serde_json::Value::Object(Default::default()),
        });
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn assistant_message_from_ndjson() {
        let line = r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"Hi!"}],"model":"claude-opus-4-5","stop_reason":"end_turn","usage":{"input_tokens":5,"output_tokens":3}},"session_id":"s1"}"#;
        let msg: Message = serde_json::from_str(line).unwrap();
        match &msg {
            Message::Assistant(a) => {
                assert_eq!(a.message.id, "m1");
                assert_eq!(a.message.content.len(), 1);
                assert_eq!(a.message.usage.input_tokens, 5);
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn assistant_message_text_helper() {
        let msg = Message::Assistant(AssistantMessage {
            message: AssistantMessageInner {
                id: "x".into(),
                content: vec![
                    ContentBlock::Text(TextBlock {
                        text: "line one".into(),
                    }),
                    ContentBlock::Text(TextBlock {
                        text: "line two".into(),
                    }),
                ],
                model: String::new(),
                stop_reason: None,
                usage: Usage::default(),
            },
            session_id: None,
            extra: serde_json::Value::Object(Default::default()),
        });
        assert_eq!(msg.assistant_text(), Some("line one\nline two".into()));
    }

    #[test]
    fn non_assistant_message_text_helper_returns_none() {
        let msg = Message::System(SystemMessage {
            subtype: "init".into(),
            session_id: String::new(),
            cwd: String::new(),
            tools: vec![],
            mcp_servers: vec![],
            model: String::new(),
            extra: serde_json::Value::Object(Default::default()),
        });
        assert_eq!(msg.assistant_text(), None);
    }

    // ── UserMessage ───────────────────────────────────────────────────────────

    #[test]
    fn user_message_round_trip() {
        let msg = Message::User(UserMessage {
            message: UserMessageInner {
                role: "user".into(),
                content: serde_json::json!("What is Rust?"),
            },
            extra: serde_json::Value::Object(Default::default()),
        });
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn user_message_from_ndjson() {
        let line = r#"{"type":"user","message":{"role":"user","content":"hello"}}"#;
        let msg: Message = serde_json::from_str(line).unwrap();
        assert!(matches!(msg, Message::User(_)));
    }

    // ── ResultMessage ─────────────────────────────────────────────────────────

    #[test]
    fn result_message_round_trip() {
        let msg = Message::Result(ResultMessage {
            subtype: "success".into(),
            cost_usd: Some(0.0042),
            duration_ms: 3500,
            duration_api_ms: 3100,
            is_error: false,
            num_turns: 2,
            session_id: Some("sess-1".into()),
            total_cost_usd: Some(0.0042),
            usage: Usage {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_input_tokens: 20,
                cache_creation_input_tokens: 5,
            },
            result: Some("Task complete.".into()),
            extra: serde_json::Value::Object(Default::default()),
        });
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn result_message_from_ndjson() {
        let line = r#"{"type":"result","subtype":"success","cost_usd":0.01,"duration_ms":1000,"duration_api_ms":900,"is_error":false,"num_turns":1,"session_id":"s1","usage":{"input_tokens":50,"output_tokens":20}}"#;
        let msg: Message = serde_json::from_str(line).unwrap();
        match msg {
            Message::Result(r) => {
                assert_eq!(r.subtype, "success");
                assert!(!r.is_error);
                assert_eq!(r.num_turns, 1);
            }
            other => panic!("expected Result, got {other:?}"),
        }
    }

    #[test]
    fn result_message_is_error_flag() {
        let r = ResultMessage {
            subtype: "error".into(),
            cost_usd: None,
            duration_ms: 0,
            duration_api_ms: 0,
            is_error: true,
            num_turns: 0,
            session_id: None,
            total_cost_usd: None,
            usage: Usage::default(),
            result: None,
            extra: serde_json::Value::Object(Default::default()),
        };
        let msg = Message::Result(r);
        assert!(msg.is_error_result());
    }

    #[test]
    fn is_error_result_false_for_non_result() {
        let msg = Message::System(SystemMessage {
            subtype: "init".into(),
            session_id: String::new(),
            cwd: String::new(),
            tools: vec![],
            mcp_servers: vec![],
            model: String::new(),
            extra: serde_json::Value::Object(Default::default()),
        });
        assert!(!msg.is_error_result());
    }

    // ── Usage ─────────────────────────────────────────────────────────────────

    #[test]
    fn usage_default_is_zero() {
        let u = Usage::default();
        assert_eq!(u.input_tokens, 0);
        assert_eq!(u.output_tokens, 0);
        assert_eq!(u.cache_read_input_tokens, 0);
        assert_eq!(u.cache_creation_input_tokens, 0);
    }

    #[test]
    fn usage_total_tokens() {
        let u = Usage {
            input_tokens: 10,
            output_tokens: 20,
            ..Default::default()
        };
        assert_eq!(u.total_tokens(), 30);
    }

    #[test]
    fn usage_total_tokens_saturates_on_overflow() {
        let u = Usage {
            input_tokens: u32::MAX,
            output_tokens: 1,
            ..Default::default()
        };
        assert_eq!(u.total_tokens(), u32::MAX);
    }

    #[test]
    fn usage_merge() {
        let mut a = Usage {
            input_tokens: 10,
            output_tokens: 5,
            cache_read_input_tokens: 2,
            cache_creation_input_tokens: 1,
        };
        let b = Usage {
            input_tokens: 3,
            output_tokens: 7,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 4,
        };
        a.merge(&b);
        assert_eq!(a.input_tokens, 13);
        assert_eq!(a.output_tokens, 12);
        assert_eq!(a.cache_read_input_tokens, 2);
        assert_eq!(a.cache_creation_input_tokens, 5);
    }

    #[test]
    fn usage_round_trip() {
        let u = Usage {
            input_tokens: 100,
            output_tokens: 200,
            cache_read_input_tokens: 50,
            cache_creation_input_tokens: 10,
        };
        let json = serde_json::to_string(&u).unwrap();
        let decoded: Usage = serde_json::from_str(&json).unwrap();
        assert_eq!(u, decoded);
    }

    // ── SessionInfo ───────────────────────────────────────────────────────────

    #[test]
    fn session_info_from_init_message() {
        let sys = SystemMessage {
            subtype: "init".into(),
            session_id: "s42".into(),
            cwd: "/workspace".into(),
            tools: vec!["bash".into()],
            mcp_servers: vec![],
            model: "claude-opus-4-5".into(),
            extra: serde_json::Value::Object(Default::default()),
        };
        let info = SessionInfo::try_from(&sys).unwrap();
        assert_eq!(info.session_id, "s42");
        assert_eq!(info.cwd, "/workspace");
        assert_eq!(info.tools, ["bash"]);
    }

    #[test]
    fn session_info_rejects_non_init_subtype() {
        let sys = SystemMessage {
            subtype: "status".into(),
            session_id: "s1".into(),
            cwd: "/".into(),
            tools: vec![],
            mcp_servers: vec![],
            model: String::new(),
            extra: serde_json::Value::Object(Default::default()),
        };
        let err = SessionInfo::try_from(&sys).unwrap_err();
        assert!(
            matches!(err, crate::errors::Error::ControlProtocol(_)),
            "expected ControlProtocol error, got {err:?}"
        );
    }

    // ── Message::session_id helper ────────────────────────────────────────────

    #[test]
    fn message_session_id_system() {
        let msg = Message::System(SystemMessage {
            subtype: "init".into(),
            session_id: "s1".into(),
            cwd: String::new(),
            tools: vec![],
            mcp_servers: vec![],
            model: String::new(),
            extra: serde_json::Value::Object(Default::default()),
        });
        assert_eq!(msg.session_id(), Some("s1"));
    }

    #[test]
    fn message_session_id_result() {
        let msg = Message::Result(ResultMessage {
            subtype: String::new(),
            cost_usd: None,
            duration_ms: 0,
            duration_api_ms: 0,
            is_error: false,
            num_turns: 0,
            session_id: Some("s99".into()),
            total_cost_usd: None,
            usage: Usage::default(),
            result: None,
            extra: serde_json::Value::Object(Default::default()),
        });
        assert_eq!(msg.session_id(), Some("s99"));
    }

    #[test]
    fn message_session_id_user_is_none() {
        let msg = Message::User(UserMessage {
            message: UserMessageInner {
                role: "user".into(),
                content: serde_json::Value::Null,
            },
            extra: serde_json::Value::Object(Default::default()),
        });
        assert_eq!(msg.session_id(), None);
    }

    // ── StreamEvent ──────────────────────────────────────────────────────────

    #[test]
    fn stream_event_from_ndjson() {
        let line = r#"{"type":"stream_event","uuid":"evt-001","session_id":"s1","event":{"kind":"progress","percent":50}}"#;
        let msg: Message = serde_json::from_str(line).unwrap();
        match &msg {
            Message::StreamEvent(e) => {
                assert_eq!(e.uuid, "evt-001");
                assert_eq!(e.session_id, "s1");
                assert_eq!(e.event["kind"], "progress");
                assert_eq!(e.event["percent"], 50);
                assert!(e.parent_tool_use_id.is_none());
            }
            other => panic!("expected StreamEvent, got {other:?}"),
        }
        assert!(msg.is_stream_event());
        assert_eq!(msg.session_id(), Some("s1"));
    }

    #[test]
    fn stream_event_with_parent_tool_use_id() {
        let line = r#"{"type":"stream_event","uuid":"evt-002","session_id":"s1","event":{},"parent_tool_use_id":"tu_123"}"#;
        let msg: Message = serde_json::from_str(line).unwrap();
        if let Message::StreamEvent(e) = &msg {
            assert_eq!(e.parent_tool_use_id.as_deref(), Some("tu_123"));
        } else {
            panic!("expected StreamEvent");
        }
    }

    #[test]
    fn stream_event_round_trip() {
        let msg = Message::StreamEvent(StreamEvent {
            uuid: "evt-003".into(),
            session_id: "s2".into(),
            event: serde_json::json!({"status": "done"}),
            parent_tool_use_id: Some("tu_456".into()),
            extra: serde_json::Value::Object(Default::default()),
        });
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn is_stream_event_false_for_other() {
        let msg = Message::System(SystemMessage {
            subtype: "init".into(),
            session_id: String::new(),
            cwd: String::new(),
            tools: vec![],
            mcp_servers: vec![],
            model: String::new(),
            extra: serde_json::Value::Object(Default::default()),
        });
        assert!(!msg.is_stream_event());
    }
}
