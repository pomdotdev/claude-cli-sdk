//! Hook system for intercepting agent lifecycle events.
//!
//! Hooks allow SDK consumers to observe and modify tool executions by
//! registering callbacks for specific lifecycle events (e.g., pre/post tool
//! use). The hook system is optional — sessions without hooks behave normally.
//!
//! # Architecture
//!
//! Hooks are registered on [`ClientConfig`](crate::config::ClientConfig) as a list of
//! [`HookMatcher`] entries. Each matcher targets a specific [`HookEvent`] and
//! optionally filters by tool name. When a matching event occurs, the
//! [`HookCallback`] is invoked with the event details.
//!
//! # Timeout Enforcement
//!
//! Each hook callback is subject to a timeout. The effective timeout is
//! `HookMatcher::timeout` if set, otherwise `ClientConfig::default_hook_timeout`
//! (default: 30s). If a callback exceeds its timeout, the SDK logs a warning
//! and defaults to [`HookOutput::allow()`] (fail-open). This ensures a
//! misbehaving hook never permanently blocks a session.
//!
//! # Internal Protocol
//!
//! When hooks are registered, the SDK exchanges structured JSON messages with
//! the CLI via the control protocol. `HookRequest` and `HookResponse` are
//! the internal wire types (not exposed publicly).

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

// ── Hook Events ──────────────────────────────────────────────────────────────

/// Lifecycle events that hooks can intercept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// Before a tool is executed. Can modify input or deny execution.
    PreToolUse,
    /// After a tool completes successfully.
    PostToolUse,
    /// After a tool fails with an error.
    PostToolUseFailure,
    /// When a user prompt is submitted (before processing).
    UserPromptSubmit,
    /// When the agent session stops.
    Stop,
    /// When a subagent session stops.
    SubagentStop,
    /// Before context compaction occurs.
    PreCompact,
    /// A general notification event.
    Notification,
}

// ── Hook Matcher ─────────────────────────────────────────────────────────────

/// A hook registration that pairs a lifecycle event with a callback.
pub struct HookMatcher {
    /// The event type this hook matches.
    pub event: HookEvent,
    /// Optional tool name filter. If `Some`, only matches the named tool.
    /// If `None`, matches all tools for the given event.
    pub tool_name: Option<String>,
    /// The callback to invoke when the hook matches.
    pub callback: HookCallback,
    /// Optional timeout for the callback. If `None`, uses the default.
    pub timeout: Option<Duration>,
}

impl std::fmt::Debug for HookMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookMatcher")
            .field("event", &self.event)
            .field("tool_name", &self.tool_name)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl HookMatcher {
    /// Create a new hook matcher for the given event with the given callback.
    pub fn new(event: HookEvent, callback: HookCallback) -> Self {
        Self {
            event,
            tool_name: None,
            callback,
            timeout: None,
        }
    }

    /// Filter this hook to only match a specific tool name.
    #[must_use]
    pub fn for_tool(mut self, name: impl Into<String>) -> Self {
        self.tool_name = Some(name.into());
        self
    }

    /// Set a timeout for this hook's callback.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Returns `true` if this matcher matches the given event and tool name.
    #[must_use]
    pub fn matches(&self, event: HookEvent, tool_name: Option<&str>) -> bool {
        if self.event != event {
            return false;
        }
        match (&self.tool_name, tool_name) {
            (Some(filter), Some(name)) => filter == name,
            (Some(_), None) => false,
            (None, _) => true,
        }
    }
}

// ── Callback types ───────────────────────────────────────────────────────────

use crate::util::BoxFuture;

/// The callback function invoked when a hook matches.
///
/// Arguments:
/// - `HookInput` — details about the event
/// - `Option<String>` — session ID (if available)
/// - `HookContext` — additional context
///
/// Returns a [`HookOutput`] with the decision and optional modifications.
pub type HookCallback =
    Arc<dyn Fn(HookInput, Option<String>, HookContext) -> BoxFuture<HookOutput> + Send + Sync>;

/// Input data provided to a hook callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookInput {
    /// The lifecycle event that triggered this hook.
    pub hook_event: HookEvent,
    /// The tool name (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// The tool's input parameters (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
    /// The tool's result (for PostToolUse/PostToolUseFailure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<serde_json::Value>,
    /// The tool_use_id (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    /// Extra data for future extension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// Additional context provided to hook callbacks.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// The active session ID.
    pub session_id: Option<String>,
}

/// The output returned by a hook callback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookOutput {
    /// The hook's decision.
    pub decision: HookDecision,
    /// Human-readable reason for the decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Updated tool input (only meaningful for `PreToolUse`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    /// Extra data for future extension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// The decision a hook can make about the intercepted event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookDecision {
    /// Allow the event to proceed as normal.
    Allow,
    /// Block the tool execution (PreToolUse only).
    Block,
    /// Modify the input and continue (PreToolUse only).
    Modify,
    /// Abort the entire session.
    Abort,
}

impl HookOutput {
    /// Convenience: allow the event to proceed.
    #[must_use]
    pub fn allow() -> Self {
        Self {
            decision: HookDecision::Allow,
            reason: None,
            updated_input: None,
            extra: None,
        }
    }

    /// Convenience: block the tool execution.
    #[must_use]
    pub fn block(reason: impl Into<String>) -> Self {
        Self {
            decision: HookDecision::Block,
            reason: Some(reason.into()),
            updated_input: None,
            extra: None,
        }
    }

    /// Convenience: modify the tool input.
    #[must_use]
    pub fn modify(updated_input: serde_json::Value) -> Self {
        Self {
            decision: HookDecision::Modify,
            reason: None,
            updated_input: Some(updated_input),
            extra: None,
        }
    }

    /// Convenience: abort the session.
    #[must_use]
    pub fn abort(reason: impl Into<String>) -> Self {
        Self {
            decision: HookDecision::Abort,
            reason: Some(reason.into()),
            updated_input: None,
            extra: None,
        }
    }
}

// ── Internal protocol messages ───────────────────────────────────────────────

/// A hook request received from the CLI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct HookRequest {
    /// Unique request ID for correlation.
    pub request_id: String,
    /// The hook event.
    pub hook_event: HookEvent,
    /// Tool name (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Tool input (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
    /// Tool result (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<serde_json::Value>,
    /// Tool use ID (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
}

impl HookRequest {
    /// Convert this wire request into a [`HookInput`] suitable for callbacks.
    #[cfg(test)]
    pub fn into_hook_input(self) -> HookInput {
        HookInput {
            hook_event: self.hook_event,
            tool_name: self.tool_name,
            tool_input: self.tool_input,
            tool_result: self.tool_result,
            tool_use_id: self.tool_use_id,
            extra: None,
        }
    }

    /// Borrow this wire request as a [`HookInput`] suitable for callbacks.
    pub(crate) fn to_hook_input(&self) -> HookInput {
        HookInput {
            hook_event: self.hook_event,
            tool_name: self.tool_name.clone(),
            tool_input: self.tool_input.clone(),
            tool_result: self.tool_result.clone(),
            tool_use_id: self.tool_use_id.clone(),
            extra: None,
        }
    }
}

/// A hook response sent back to the CLI.
///
/// Shares the same `{kind, request_id, result}` wire envelope pattern as
/// [`ControlResponse`](crate::permissions::ControlResponse), but they are
/// kept separate because they carry different result types and serve different
/// protocol flows (hook lifecycle vs permission handshake). A generic
/// `ControlEnvelope<T>` was considered but rejected as over-abstraction for
/// two types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct HookResponse {
    /// The kind of response.
    pub kind: String,
    /// Must match the `request_id` from the corresponding request.
    pub request_id: String,
    /// The result payload.
    pub result: HookOutput,
}

impl HookResponse {
    /// Create a response from a hook output and the originating request ID.
    pub fn from_output(request_id: String, output: HookOutput) -> Self {
        Self {
            kind: "hook_response".into(),
            request_id,
            result: output,
        }
    }
}

// ── Hook dispatch ────────────────────────────────────────────────────────────

/// Dispatch a hook request to matching callbacks with timeout enforcement.
///
/// If a matching callback times out, the hook defaults to `HookOutput::allow()`
/// (fail-open). This ensures a misbehaving hook never permanently blocks a
/// session.
pub(crate) async fn dispatch_hook(
    req: &HookRequest,
    hooks: &[HookMatcher],
    default_hook_timeout: Duration,
    session_id: Option<String>,
) -> HookOutput {
    let input = req.to_hook_input();

    for matcher in hooks {
        if !matcher.matches(req.hook_event, req.tool_name.as_deref()) {
            continue;
        }

        let effective_timeout = matcher.timeout.unwrap_or(default_hook_timeout);
        let ctx = HookContext {
            session_id: session_id.clone(),
        };

        let fut = (matcher.callback)(input.clone(), session_id.clone(), ctx);
        match tokio::time::timeout(effective_timeout, fut).await {
            Ok(output) => return output,
            Err(_) => {
                tracing::warn!(
                    event = ?req.hook_event,
                    tool = ?req.tool_name,
                    timeout_secs = effective_timeout.as_secs_f64(),
                    "hook callback timed out, defaulting to allow (fail-open)"
                );
                return HookOutput::allow();
            }
        }
    }

    // No matching hook — allow by default.
    HookOutput::allow()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_event_round_trip() {
        let events = [
            HookEvent::PreToolUse,
            HookEvent::PostToolUse,
            HookEvent::PostToolUseFailure,
            HookEvent::UserPromptSubmit,
            HookEvent::Stop,
            HookEvent::SubagentStop,
            HookEvent::PreCompact,
            HookEvent::Notification,
        ];
        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let decoded: HookEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, decoded, "round-trip failed for {event:?}");
        }
    }

    #[test]
    fn hook_matcher_matches_any_tool() {
        let cb: HookCallback = Arc::new(|_, _, _| Box::pin(async { HookOutput::allow() }));
        let matcher = HookMatcher::new(HookEvent::PreToolUse, cb);
        assert!(matcher.matches(HookEvent::PreToolUse, Some("bash")));
        assert!(matcher.matches(HookEvent::PreToolUse, Some("read_file")));
        assert!(matcher.matches(HookEvent::PreToolUse, None));
        assert!(!matcher.matches(HookEvent::PostToolUse, Some("bash")));
    }

    #[test]
    fn hook_matcher_matches_specific_tool() {
        let cb: HookCallback = Arc::new(|_, _, _| Box::pin(async { HookOutput::allow() }));
        let matcher = HookMatcher::new(HookEvent::PreToolUse, cb).for_tool("bash");
        assert!(matcher.matches(HookEvent::PreToolUse, Some("bash")));
        assert!(!matcher.matches(HookEvent::PreToolUse, Some("read_file")));
        assert!(!matcher.matches(HookEvent::PreToolUse, None));
    }

    #[test]
    fn hook_matcher_with_timeout() {
        let cb: HookCallback = Arc::new(|_, _, _| Box::pin(async { HookOutput::allow() }));
        let matcher = HookMatcher::new(HookEvent::Stop, cb).with_timeout(Duration::from_secs(5));
        assert_eq!(matcher.timeout, Some(Duration::from_secs(5)));
    }

    #[test]
    fn hook_output_allow() {
        let output = HookOutput::allow();
        assert_eq!(output.decision, HookDecision::Allow);
        assert!(output.reason.is_none());
    }

    #[test]
    fn hook_output_block() {
        let output = HookOutput::block("dangerous command");
        assert_eq!(output.decision, HookDecision::Block);
        assert_eq!(output.reason.as_deref(), Some("dangerous command"));
    }

    #[test]
    fn hook_output_modify() {
        let output = HookOutput::modify(serde_json::json!({"safe": true}));
        assert_eq!(output.decision, HookDecision::Modify);
        assert!(output.updated_input.is_some());
    }

    #[test]
    fn hook_output_abort() {
        let output = HookOutput::abort("critical failure");
        assert_eq!(output.decision, HookDecision::Abort);
        assert_eq!(output.reason.as_deref(), Some("critical failure"));
    }

    #[test]
    fn hook_output_round_trip() {
        let output = HookOutput {
            decision: HookDecision::Modify,
            reason: Some("safety".into()),
            updated_input: Some(serde_json::json!({"command": "ls"})),
            extra: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        let decoded: HookOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(output.decision, decoded.decision);
        assert_eq!(output.reason, decoded.reason);
        assert_eq!(output.updated_input, decoded.updated_input);
    }

    #[test]
    fn hook_request_round_trip() {
        let req = HookRequest {
            request_id: "hr-1".into(),
            hook_event: HookEvent::PreToolUse,
            tool_name: Some("bash".into()),
            tool_input: Some(serde_json::json!({"command": "echo hello"})),
            tool_result: None,
            tool_use_id: Some("tu-1".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: HookRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn hook_request_into_hook_input() {
        let req = HookRequest {
            request_id: "hr-1".into(),
            hook_event: HookEvent::PostToolUse,
            tool_name: Some("bash".into()),
            tool_input: None,
            tool_result: Some(serde_json::json!("output")),
            tool_use_id: Some("tu-1".into()),
        };
        let input = req.into_hook_input();
        assert_eq!(input.hook_event, HookEvent::PostToolUse);
        assert_eq!(input.tool_name.as_deref(), Some("bash"));
        assert!(input.tool_result.is_some());
    }

    #[test]
    fn hook_response_from_output() {
        let output = HookOutput::allow();
        let resp = HookResponse::from_output("req-1".into(), output);
        assert_eq!(resp.kind, "hook_response");
        assert_eq!(resp.request_id, "req-1");
        assert_eq!(resp.result.decision, HookDecision::Allow);
    }

    #[test]
    fn hook_response_round_trip() {
        let resp = HookResponse {
            kind: "hook_response".into(),
            request_id: "hr-1".into(),
            result: HookOutput::block("no"),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: HookResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn hook_decision_serde() {
        let decisions = [
            (HookDecision::Allow, r#""allow""#),
            (HookDecision::Block, r#""block""#),
            (HookDecision::Modify, r#""modify""#),
            (HookDecision::Abort, r#""abort""#),
        ];
        for (decision, expected_json) in decisions {
            let json = serde_json::to_string(&decision).unwrap();
            assert_eq!(json, expected_json);
            let decoded: HookDecision = serde_json::from_str(&json).unwrap();
            assert_eq!(decision, decoded);
        }
    }

    #[test]
    fn hook_input_optional_fields() {
        // Minimal input with only required fields.
        let json = r#"{"hook_event":"stop"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hook_event, HookEvent::Stop);
        assert!(input.tool_name.is_none());
        assert!(input.tool_input.is_none());
        assert!(input.tool_result.is_none());
    }

    // ── Hook dispatch tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn hook_timeout_defaults_to_config_value() {
        let cb: HookCallback =
            Arc::new(|_, _, _| Box::pin(async { HookOutput::block("should arrive") }));
        let matchers = vec![HookMatcher::new(HookEvent::PreToolUse, cb)];

        let req = HookRequest {
            request_id: "r1".into(),
            hook_event: HookEvent::PreToolUse,
            tool_name: Some("Bash".into()),
            tool_input: None,
            tool_result: None,
            tool_use_id: None,
        };

        let output = dispatch_hook(&req, &matchers, Duration::from_secs(30), None).await;
        assert_eq!(output.decision, HookDecision::Block);
    }

    #[tokio::test]
    async fn hook_timeout_override() {
        let cb: HookCallback =
            Arc::new(|_, _, _| Box::pin(async { HookOutput::block("custom timeout") }));
        let matchers =
            vec![HookMatcher::new(HookEvent::PreToolUse, cb).with_timeout(Duration::from_secs(60))];

        let req = HookRequest {
            request_id: "r1".into(),
            hook_event: HookEvent::PreToolUse,
            tool_name: None,
            tool_input: None,
            tool_result: None,
            tool_use_id: None,
        };

        // Should use the per-matcher timeout (60s), not default (1ms).
        let output = dispatch_hook(&req, &matchers, Duration::from_millis(1), None).await;
        assert_eq!(output.decision, HookDecision::Block);
    }

    #[tokio::test]
    async fn hook_timeout_fires_returns_allow() {
        // Hook that sleeps forever.
        let cb: HookCallback = Arc::new(|_, _, _| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                HookOutput::block("never reached")
            })
        });
        let matchers = vec![HookMatcher::new(HookEvent::PreToolUse, cb)];

        let req = HookRequest {
            request_id: "r1".into(),
            hook_event: HookEvent::PreToolUse,
            tool_name: Some("Bash".into()),
            tool_input: None,
            tool_result: None,
            tool_use_id: None,
        };

        let output = dispatch_hook(&req, &matchers, Duration::from_millis(10), None).await;
        // Fail-open: timed out hook should default to Allow.
        assert_eq!(output.decision, HookDecision::Allow);
    }
}
