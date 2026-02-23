//! Permission types and the `can_use_tool` callback pattern.
//!
//! When the CLI requests permission to use a tool, the SDK can intercept the
//! request via a [`CanUseToolCallback`].  The callback receives tool metadata
//! and returns a [`PermissionDecision`] (allow or deny).
//!
//! This module also defines the internal control protocol messages exchanged
//! with the CLI over stdin/stdout for permission handshakes.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::util::BoxFuture;

// ── Public types ─────────────────────────────────────────────────────────────

/// The decision made by a permission callback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum PermissionDecision {
    /// Allow the tool to execute, optionally with modified input.
    Allow {
        /// If `Some`, the tool input will be replaced with this value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        updated_input: Option<serde_json::Value>,
    },
    /// Deny the tool execution.
    Deny {
        /// Human-readable reason for denial (shown to the model).
        message: String,
        /// If `true`, abort the entire session rather than just this tool call.
        #[serde(default)]
        interrupt: bool,
    },
}

impl PermissionDecision {
    /// Convenience: allow with no input modification.
    #[must_use]
    pub fn allow() -> Self {
        Self::Allow {
            updated_input: None,
        }
    }

    /// Convenience: allow with modified input.
    #[must_use]
    pub fn allow_with_input(input: serde_json::Value) -> Self {
        Self::Allow {
            updated_input: Some(input),
        }
    }

    /// Convenience: deny with a message but don't interrupt.
    #[must_use]
    pub fn deny(message: impl Into<String>) -> Self {
        Self::Deny {
            message: message.into(),
            interrupt: false,
        }
    }

    /// Convenience: deny and interrupt the session.
    #[must_use]
    pub fn deny_and_interrupt(message: impl Into<String>) -> Self {
        Self::Deny {
            message: message.into(),
            interrupt: true,
        }
    }
}

/// Context provided to the permission callback.
#[derive(Debug, Clone)]
pub struct PermissionContext {
    /// The tool_use ID from the model's request.
    pub tool_use_id: String,
    /// The active session ID.
    pub session_id: String,
    /// The request ID from the control protocol.
    pub request_id: String,
    /// Suggested permission actions from the CLI.
    pub suggestions: Vec<String>,
}

/// Callback invoked when the CLI requests permission to use a tool.
///
/// Arguments:
/// - `&str` — tool name
/// - `&Value` — tool input (JSON)
/// - `PermissionContext` — additional context
///
/// Must return a [`PermissionDecision`].
pub type CanUseToolCallback = Arc<
    dyn Fn(&str, &serde_json::Value, PermissionContext) -> BoxFuture<PermissionDecision>
        + Send
        + Sync,
>;

// ── Internal control protocol messages ───────────────────────────────────────

/// A permission request received from the CLI via the control protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ControlRequest {
    /// Unique request ID for correlation.
    pub request_id: String,
    /// The actual request payload.
    pub request: ControlRequestData,
}

/// The payload of a control request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ControlRequestData {
    /// Permission to use a tool.
    PermissionRequest {
        tool_name: String,
        tool_input: serde_json::Value,
        tool_use_id: String,
        #[serde(default)]
        suggestions: Vec<String>,
    },
}

/// A permission response sent back to the CLI.
///
/// Shares the same `{kind, request_id, result}` wire envelope pattern as
/// [`HookResponse`](crate::hooks::HookResponse), but they are kept separate
/// because they carry different result types and serve different protocol flows
/// (permission handshake vs hook lifecycle). A generic `ControlEnvelope<T>`
/// was considered but rejected as over-abstraction for two types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ControlResponse {
    /// The kind of response.
    pub kind: String,
    /// Must match the `request_id` from the corresponding request.
    pub request_id: String,
    /// The result payload.
    pub result: ControlResponseResult,
}

/// The result payload of a control response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ControlResponseResult {
    /// Grant permission.
    Allow {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        updated_input: Option<serde_json::Value>,
    },
    /// Deny permission.
    Deny {
        message: String,
        #[serde(default)]
        interrupt: bool,
    },
}

impl From<PermissionDecision> for ControlResponseResult {
    fn from(decision: PermissionDecision) -> Self {
        match decision {
            PermissionDecision::Allow { updated_input } => Self::Allow { updated_input },
            PermissionDecision::Deny { message, interrupt } => Self::Deny { message, interrupt },
        }
    }
}

/// A legacy permission request format (pre-control-protocol).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LegacyPermissionRequest {
    /// Unique request ID.
    pub request_id: String,
    /// Tool being requested.
    pub tool_name: String,
    /// Action description (e.g., "execute bash command").
    pub action: String,
    /// Human-readable details about the request.
    #[serde(default)]
    pub details: Option<String>,
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_decision_allow_round_trip() {
        let d = PermissionDecision::allow();
        let json = serde_json::to_string(&d).unwrap();
        let decoded: PermissionDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(d, decoded);
    }

    #[test]
    fn permission_decision_allow_with_input() {
        let d = PermissionDecision::allow_with_input(serde_json::json!({"modified": true}));
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("modified"));
        let decoded: PermissionDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(d, decoded);
    }

    #[test]
    fn permission_decision_deny_round_trip() {
        let d = PermissionDecision::deny("not allowed");
        let json = serde_json::to_string(&d).unwrap();
        let decoded: PermissionDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(d, decoded);
    }

    #[test]
    fn permission_decision_deny_interrupt() {
        let d = PermissionDecision::deny_and_interrupt("abort");
        if let PermissionDecision::Deny { interrupt, .. } = &d {
            assert!(interrupt);
        } else {
            panic!("expected Deny");
        }
    }

    #[test]
    fn control_request_permission_round_trip() {
        let req = ControlRequest {
            request_id: "req-1".into(),
            request: ControlRequestData::PermissionRequest {
                tool_name: "bash".into(),
                tool_input: serde_json::json!({"command": "rm -rf /"}),
                tool_use_id: "tu-1".into(),
                suggestions: vec!["allow_once".into()],
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: ControlRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn control_response_allow_round_trip() {
        let resp = ControlResponse {
            kind: "permission_response".into(),
            request_id: "req-1".into(),
            result: ControlResponseResult::Allow {
                updated_input: None,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: ControlResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn control_response_deny_round_trip() {
        let resp = ControlResponse {
            kind: "permission_response".into(),
            request_id: "req-1".into(),
            result: ControlResponseResult::Deny {
                message: "dangerous".into(),
                interrupt: true,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: ControlResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn permission_decision_to_control_response_result() {
        let allow = PermissionDecision::allow();
        let result: ControlResponseResult = allow.into();
        assert!(matches!(result, ControlResponseResult::Allow { .. }));

        let deny = PermissionDecision::deny("no");
        let result: ControlResponseResult = deny.into();
        assert!(matches!(result, ControlResponseResult::Deny { .. }));
    }

    #[test]
    fn legacy_permission_request_round_trip() {
        let req = LegacyPermissionRequest {
            request_id: "lr-1".into(),
            tool_name: "read_file".into(),
            action: "read /etc/passwd".into(),
            details: Some("attempting to read sensitive file".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: LegacyPermissionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }
}
