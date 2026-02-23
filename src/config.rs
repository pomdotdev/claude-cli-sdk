//! Client configuration — `ClientConfig` with typed builder pattern.
//!
//! [`ClientConfig`] carries every option needed to spawn and control a Claude
//! Code CLI session. It uses [`typed_builder`] so that required fields must be
//! supplied at compile time while optional fields have sensible defaults.
//!
//! # Example
//!
//! ```rust
//! use claude_cli_sdk::config::{ClientConfig, PermissionMode};
//!
//! let config = ClientConfig::builder()
//!     .prompt("List files in /tmp")
//!     .build();
//! ```

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use tokio_util::sync::CancellationToken;

use crate::callback::MessageCallback;
use crate::hooks::HookMatcher;
use crate::mcp::McpServers;
use crate::permissions::CanUseToolCallback;

// ── ClientConfig ─────────────────────────────────────────────────────────────

/// Configuration for a Claude Code SDK client session.
///
/// Use [`ClientConfig::builder()`] to construct.
#[derive(TypedBuilder)]
pub struct ClientConfig {
    // ── Required ─────────────────────────────────────────────────────────
    /// The prompt text to send to Claude.
    #[builder(setter(into))]
    pub prompt: String,

    // ── Session ──────────────────────────────────────────────────────────
    /// Path to the Claude CLI binary. If `None`, auto-discovered via
    /// [`find_cli()`](crate::discovery::find_cli).
    #[builder(default, setter(strip_option))]
    pub cli_path: Option<PathBuf>,

    /// Working directory for the Claude process.
    #[builder(default, setter(strip_option))]
    pub cwd: Option<PathBuf>,

    /// Model to use (e.g., `"claude-sonnet-4-5"`).
    #[builder(default, setter(strip_option, into))]
    pub model: Option<String>,

    /// Fallback model if the primary is unavailable.
    #[builder(default, setter(strip_option, into))]
    pub fallback_model: Option<String>,

    /// System prompt configuration.
    #[builder(default, setter(strip_option))]
    pub system_prompt: Option<SystemPrompt>,

    // ── Limits ───────────────────────────────────────────────────────────
    /// Maximum number of agentic turns before stopping.
    #[builder(default, setter(strip_option))]
    pub max_turns: Option<u32>,

    /// Maximum USD budget for the session.
    #[builder(default, setter(strip_option))]
    pub max_budget_usd: Option<f64>,

    /// Maximum thinking tokens per turn.
    #[builder(default, setter(strip_option))]
    pub max_thinking_tokens: Option<u32>,

    // ── Tools ────────────────────────────────────────────────────────────
    /// Explicitly allowed tool names.
    #[builder(default)]
    pub allowed_tools: Vec<String>,

    /// Explicitly disallowed tool names.
    #[builder(default)]
    pub disallowed_tools: Vec<String>,

    // ── Permissions ──────────────────────────────────────────────────────
    /// Permission mode for the session.
    #[builder(default)]
    pub permission_mode: PermissionMode,

    /// Callback invoked when the CLI requests tool use permission.
    #[builder(default, setter(strip_option))]
    pub can_use_tool: Option<CanUseToolCallback>,

    // ── Session management ───────────────────────────────────────────────
    /// Resume an existing session by ID.
    #[builder(default, setter(strip_option, into))]
    pub resume: Option<String>,

    // ── Hooks ────────────────────────────────────────────────────────────
    /// Lifecycle hooks to register for the session.
    #[builder(default)]
    pub hooks: Vec<HookMatcher>,

    // ── MCP ──────────────────────────────────────────────────────────────
    /// External MCP server configurations.
    #[builder(default)]
    pub mcp_servers: McpServers,

    // ── Callback ─────────────────────────────────────────────────────────
    /// Optional message callback for observe/filter.
    #[builder(default, setter(strip_option))]
    pub message_callback: Option<MessageCallback>,

    // ── Environment ──────────────────────────────────────────────────────
    /// Extra environment variables to pass to the CLI process.
    #[builder(default)]
    pub env: HashMap<String, String>,

    /// Enable verbose (debug) output from the CLI.
    #[builder(default)]
    pub verbose: bool,

    // ── Output ───────────────────────────────────────────────────────────
    /// Output format. Defaults to `"stream-json"` for SDK use.
    ///
    /// `"stream-json"` enables realtime streaming — the CLI outputs NDJSON
    /// lines as events happen. This is required for the SDK's init handshake
    /// and multi-turn conversations.
    ///
    /// Other options: `"json"` (single result blob), `"text"` (human-readable).
    #[builder(default_code = r#""stream-json".into()"#, setter(into))]
    pub output_format: String,

    // ── Extra CLI args ────────────────────────────────────────────────────
    /// Arbitrary extra CLI flags to pass through to the Claude process.
    ///
    /// Keys are flag names (without the `--` prefix). Values are optional:
    /// - `Some("value")` produces `--key value`
    /// - `None` produces `--key` (boolean-style flag)
    ///
    /// Uses [`BTreeMap`] to guarantee deterministic CLI arg ordering across
    /// invocations (important for reproducible test snapshots).
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::collections::BTreeMap;
    /// use claude_cli_sdk::ClientConfig;
    ///
    /// let config = ClientConfig::builder()
    ///     .prompt("hello")
    ///     .extra_args(BTreeMap::from([
    ///         ("replay-user-messages".into(), None),
    ///         ("context-window".into(), Some("200000".into())),
    ///     ]))
    ///     .build();
    /// ```
    #[builder(default)]
    pub extra_args: BTreeMap<String, Option<String>>,

    // ── Timeouts ──────────────────────────────────────────────────────────
    /// Deadline for process spawn + init message. `None` = wait forever.
    ///
    /// Default: `Some(30s)`.
    #[builder(default_code = "Some(Duration::from_secs(30))")]
    pub connect_timeout: Option<Duration>,

    /// Deadline for `child.wait()` during close. On expiry the process is
    /// killed. `None` = wait forever.
    ///
    /// Default: `Some(10s)`.
    #[builder(default_code = "Some(Duration::from_secs(10))")]
    pub close_timeout: Option<Duration>,

    /// If `true`, close stdin immediately after spawning the CLI process.
    ///
    /// This is required when using `--print` mode (the default) because
    /// the CLI expects stdin EOF before processing the prompt with
    /// `--output-format stream-json`.
    ///
    /// Default: `true`.
    #[builder(default_code = "true")]
    pub end_input_on_connect: bool,

    /// Per-message recv deadline. `None` = wait forever (default).
    ///
    /// This is for detecting hung/zombie processes, not for limiting turn
    /// time — Claude turns can legitimately take minutes.
    #[builder(default)]
    pub read_timeout: Option<Duration>,

    /// Fallback timeout for hook callbacks when [`HookMatcher::timeout`] is
    /// `None`.
    ///
    /// Default: 30 seconds.
    #[builder(default_code = "Duration::from_secs(30)")]
    pub default_hook_timeout: Duration,

    /// Deadline for the `--version` check in
    /// [`check_cli_version()`](crate::discovery::check_cli_version).
    /// `None` = wait forever.
    ///
    /// Default: `Some(5s)`.
    #[builder(default_code = "Some(Duration::from_secs(5))")]
    pub version_check_timeout: Option<Duration>,

    /// Deadline for control requests (e.g., `set_model`, `set_permission_mode`).
    /// If the CLI does not respond within this duration, the request fails with
    /// [`Error::Timeout`](crate::Error::Timeout).
    ///
    /// Default: `30s`.
    #[builder(default_code = "Duration::from_secs(30)")]
    pub control_request_timeout: Duration,

    // ── Cancellation ─────────────────────────────────────────────────────
    /// Optional cancellation token for cooperative cancellation.
    ///
    /// When cancelled, in-flight streams yield [`Error::Cancelled`](crate::Error::Cancelled)
    /// and the background reader task shuts down cleanly.
    #[builder(default, setter(strip_option))]
    pub cancellation_token: Option<CancellationToken>,

    // ── Stderr ───────────────────────────────────────────────────────────
    /// Optional callback for CLI stderr output.
    #[builder(default, setter(strip_option))]
    pub stderr_callback: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("prompt", &self.prompt)
            .field("cli_path", &self.cli_path)
            .field("cwd", &self.cwd)
            .field("model", &self.model)
            .field("permission_mode", &self.permission_mode)
            .field("max_turns", &self.max_turns)
            .field("max_budget_usd", &self.max_budget_usd)
            .field("verbose", &self.verbose)
            .field("output_format", &self.output_format)
            .field("connect_timeout", &self.connect_timeout)
            .field("close_timeout", &self.close_timeout)
            .field("read_timeout", &self.read_timeout)
            .field("default_hook_timeout", &self.default_hook_timeout)
            .field("version_check_timeout", &self.version_check_timeout)
            .field("control_request_timeout", &self.control_request_timeout)
            .finish_non_exhaustive()
    }
}

// ── PermissionMode ───────────────────────────────────────────────────────────

/// Permission mode controlling how tool use requests are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Default: prompt the user for each tool use.
    #[default]
    Default,
    /// Automatically accept file edits (still prompt for other tools).
    AcceptEdits,
    /// Plan-only mode: suggest changes but don't execute.
    Plan,
    /// Bypass all permission prompts (dangerous — use in CI only).
    BypassPermissions,
}

impl PermissionMode {
    /// Convert to the CLI flag value.
    #[must_use]
    pub fn as_cli_flag(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::Plan => "plan",
            Self::BypassPermissions => "bypassPermissions",
        }
    }
}

// ── SystemPrompt ─────────────────────────────────────────────────────────────

/// System prompt configuration for a Claude session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SystemPrompt {
    /// A raw text system prompt.
    Text {
        /// The system prompt text.
        text: String,
    },
    /// A named preset system prompt.
    Preset {
        /// The preset kind (e.g., `"custom"`).
        kind: String,
        /// The preset name.
        preset: String,
        /// Additional text to append after the preset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        append: Option<String>,
    },
}

impl SystemPrompt {
    /// Create a text system prompt.
    #[must_use]
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text { text: s.into() }
    }

    /// Create a preset system prompt.
    #[must_use]
    pub fn preset(kind: impl Into<String>, preset: impl Into<String>) -> Self {
        Self::Preset {
            kind: kind.into(),
            preset: preset.into(),
            append: None,
        }
    }
}

// ── CLI arg generation ───────────────────────────────────────────────────────

impl ClientConfig {
    /// Validate the configuration, returning an error for invalid settings.
    ///
    /// Checks:
    /// - If `cwd` is set, it must exist and be a directory.
    ///
    /// This is called automatically by [`Client::new()`](crate::Client::new).
    pub fn validate(&self) -> crate::errors::Result<()> {
        if let Some(ref cwd) = self.cwd {
            if !cwd.exists() {
                return Err(crate::errors::Error::Config(format!(
                    "working directory does not exist: {}",
                    cwd.display()
                )));
            }
            if !cwd.is_dir() {
                return Err(crate::errors::Error::Config(format!(
                    "working directory is not a directory: {}",
                    cwd.display()
                )));
            }
        }
        Ok(())
    }

    /// Build the CLI argument list for spawning the Claude process.
    ///
    /// This does NOT include the binary path itself — just the arguments.
    #[must_use]
    pub fn to_cli_args(&self) -> Vec<String> {
        let mut args = vec![
            "--output-format".into(),
            self.output_format.clone(),
            "--print".into(),
            self.prompt.clone(),
        ];

        // stream-json output requires --verbose and benefits from
        // --input-format stream-json for bidirectional streaming.
        if self.output_format == "stream-json" && !self.verbose {
            args.push("--verbose".into());
        }

        if let Some(model) = &self.model {
            args.push("--model".into());
            args.push(model.clone());
        }

        if let Some(fallback) = &self.fallback_model {
            args.push("--fallback-model".into());
            args.push(fallback.clone());
        }

        if let Some(turns) = self.max_turns {
            args.push("--max-turns".into());
            args.push(turns.to_string());
        }

        if let Some(budget) = self.max_budget_usd {
            args.push("--max-budget-usd".into());
            args.push(budget.to_string());
        }

        if let Some(thinking) = self.max_thinking_tokens {
            args.push("--max-thinking-tokens".into());
            args.push(thinking.to_string());
        }

        if self.permission_mode != PermissionMode::Default {
            args.push("--permission-mode".into());
            args.push(self.permission_mode.as_cli_flag().into());
        }

        if let Some(resume) = &self.resume {
            args.push("--resume".into());
            args.push(resume.clone());
        }

        if self.verbose {
            args.push("--verbose".into());
        }

        for tool in &self.allowed_tools {
            args.push("--allowedTools".into());
            args.push(tool.clone());
        }

        for tool in &self.disallowed_tools {
            args.push("--disallowedTools".into());
            args.push(tool.clone());
        }

        if !self.mcp_servers.is_empty() {
            let json = serde_json::to_string(&self.mcp_servers)
                .expect("McpServers serialization is infallible");
            args.push("--mcp-servers".into());
            args.push(json);
        }

        if let Some(prompt) = &self.system_prompt {
            match prompt {
                SystemPrompt::Text { text } => {
                    args.push("--system-prompt".into());
                    args.push(text.clone());
                }
                SystemPrompt::Preset { preset, append, .. } => {
                    args.push("--system-prompt-preset".into());
                    args.push(preset.clone());
                    if let Some(append_text) = append {
                        args.push("--append-system-prompt".into());
                        args.push(append_text.clone());
                    }
                }
            }
        }

        // Extra args — appended last so they can override anything above.
        for (key, value) in &self.extra_args {
            args.push(format!("--{key}"));
            if let Some(v) = value {
                args.push(v.clone());
            }
        }

        args
    }

    /// Build the environment variable map for the CLI process.
    ///
    /// Merges SDK defaults with `self.env` (user-supplied env takes precedence)
    /// and any SDK-internal env vars.
    ///
    /// SDK defaults:
    /// - `CLAUDE_CODE_SDK_ORIGINATOR=claude_cli_sdk_rs` — telemetry originator
    /// - `CI=true` — headless mode (suppress interactive prompts)
    /// - `TERM=dumb` — prevent ANSI escape sequences in output
    #[must_use]
    pub fn to_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();

        // SDK defaults (overridable by self.env)
        env.insert(
            "CLAUDE_CODE_SDK_ORIGINATOR".into(),
            "claude_cli_sdk_rs".into(),
        );
        env.insert("CI".into(), "true".into());
        env.insert("TERM".into(), "dumb".into());

        // User-supplied env overrides defaults
        env.extend(self.env.clone());

        // Control protocol (always set if needed, cannot be overridden)
        if self.can_use_tool.is_some() || !self.hooks.is_empty() {
            env.insert("CLAUDE_CODE_SDK_CONTROL_PORT".into(), "stdin".into());
        }

        env
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_minimal() {
        let config = ClientConfig::builder().prompt("hello").build();
        assert_eq!(config.prompt, "hello");
        assert_eq!(config.output_format, "stream-json");
        assert_eq!(config.permission_mode, PermissionMode::Default);
    }

    #[test]
    fn builder_full() {
        let config = ClientConfig::builder()
            .prompt("test prompt")
            .model("claude-opus-4-5")
            .max_turns(5_u32)
            .max_budget_usd(1.0_f64)
            .permission_mode(PermissionMode::AcceptEdits)
            .verbose(true)
            .build();

        assert_eq!(config.model.as_deref(), Some("claude-opus-4-5"));
        assert_eq!(config.max_turns, Some(5));
        assert_eq!(config.max_budget_usd, Some(1.0));
        assert_eq!(config.permission_mode, PermissionMode::AcceptEdits);
        assert!(config.verbose);
    }

    #[test]
    fn to_cli_args_minimal() {
        let config = ClientConfig::builder().prompt("hello").build();
        let args = config.to_cli_args();
        assert!(args.contains(&"--output-format".into()));
        assert!(args.contains(&"stream-json".into()));
        assert!(args.contains(&"--print".into()));
        assert!(args.contains(&"hello".into()));
    }

    #[test]
    fn to_cli_args_with_model_and_turns() {
        let config = ClientConfig::builder()
            .prompt("test")
            .model("claude-sonnet-4-5")
            .max_turns(10_u32)
            .build();
        let args = config.to_cli_args();
        assert!(args.contains(&"--model".into()));
        assert!(args.contains(&"claude-sonnet-4-5".into()));
        assert!(args.contains(&"--max-turns".into()));
        assert!(args.contains(&"10".into()));
    }

    #[test]
    fn to_cli_args_with_permission_mode() {
        let config = ClientConfig::builder()
            .prompt("test")
            .permission_mode(PermissionMode::BypassPermissions)
            .build();
        let args = config.to_cli_args();
        assert!(args.contains(&"--permission-mode".into()));
        assert!(args.contains(&"bypassPermissions".into()));
    }

    #[test]
    fn to_cli_args_default_permission_mode_not_included() {
        let config = ClientConfig::builder().prompt("test").build();
        let args = config.to_cli_args();
        assert!(!args.contains(&"--permission-mode".into()));
    }

    #[test]
    fn to_cli_args_with_system_prompt_text() {
        let config = ClientConfig::builder()
            .prompt("test")
            .system_prompt(SystemPrompt::text("You are a helpful assistant"))
            .build();
        let args = config.to_cli_args();
        assert!(args.contains(&"--system-prompt".into()));
        assert!(args.contains(&"You are a helpful assistant".into()));
    }

    #[test]
    fn to_cli_args_with_mcp_servers() {
        use crate::mcp::McpServerConfig;

        let mut servers = McpServers::new();
        servers.insert(
            "fs".into(),
            McpServerConfig::new("npx").with_args(["-y", "mcp-fs"]),
        );

        let config = ClientConfig::builder()
            .prompt("test")
            .mcp_servers(servers)
            .build();
        let args = config.to_cli_args();
        assert!(args.contains(&"--mcp-servers".into()));
    }

    #[test]
    fn to_env_without_callbacks() {
        let config = ClientConfig::builder().prompt("test").build();
        let env = config.to_env();
        assert!(!env.contains_key("CLAUDE_CODE_SDK_CONTROL_PORT"));
    }

    #[test]
    fn to_env_includes_originator_and_headless_defaults() {
        let config = ClientConfig::builder().prompt("test").build();
        let env = config.to_env();
        assert_eq!(
            env.get("CLAUDE_CODE_SDK_ORIGINATOR"),
            Some(&"claude_cli_sdk_rs".into())
        );
        assert_eq!(env.get("CI"), Some(&"true".into()));
        assert_eq!(env.get("TERM"), Some(&"dumb".into()));
    }

    #[test]
    fn to_env_user_env_overrides_defaults() {
        let config = ClientConfig::builder()
            .prompt("test")
            .env(HashMap::from([
                ("CI".into(), "false".into()),
                ("TERM".into(), "xterm-256color".into()),
            ]))
            .build();
        let env = config.to_env();
        // User-supplied values should override SDK defaults.
        assert_eq!(env.get("CI"), Some(&"false".into()));
        assert_eq!(env.get("TERM"), Some(&"xterm-256color".into()));
        // Originator should still be present (not overridden).
        assert_eq!(
            env.get("CLAUDE_CODE_SDK_ORIGINATOR"),
            Some(&"claude_cli_sdk_rs".into())
        );
    }

    #[test]
    fn to_env_with_hooks_enables_control_port() {
        use crate::hooks::{HookCallback, HookEvent, HookMatcher, HookOutput};
        let cb: HookCallback = Arc::new(|_, _, _| Box::pin(async { HookOutput::allow() }));
        let config = ClientConfig::builder()
            .prompt("test")
            .hooks(vec![HookMatcher::new(HookEvent::PreToolUse, cb)])
            .build();
        let env = config.to_env();
        assert_eq!(
            env.get("CLAUDE_CODE_SDK_CONTROL_PORT"),
            Some(&"stdin".into())
        );
    }

    #[test]
    fn permission_mode_serde_round_trip() {
        let modes = [
            PermissionMode::Default,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
            PermissionMode::BypassPermissions,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let decoded: PermissionMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, decoded);
        }
    }

    #[test]
    fn system_prompt_text_round_trip() {
        let sp = SystemPrompt::text("You are helpful");
        let json = serde_json::to_string(&sp).unwrap();
        let decoded: SystemPrompt = serde_json::from_str(&json).unwrap();
        assert_eq!(sp, decoded);
    }

    #[test]
    fn system_prompt_preset_round_trip() {
        let sp = SystemPrompt::Preset {
            kind: "custom".into(),
            preset: "coding".into(),
            append: Some("Also be concise.".into()),
        };
        let json = serde_json::to_string(&sp).unwrap();
        let decoded: SystemPrompt = serde_json::from_str(&json).unwrap();
        assert_eq!(sp, decoded);
    }

    #[test]
    fn debug_does_not_panic() {
        let config = ClientConfig::builder().prompt("test").build();
        let _ = format!("{config:?}");
    }

    #[test]
    fn to_cli_args_with_allowed_tools() {
        let config = ClientConfig::builder()
            .prompt("test")
            .allowed_tools(vec!["bash".into(), "read_file".into()])
            .build();
        let args = config.to_cli_args();
        let allowed_count = args.iter().filter(|a| *a == "--allowedTools").count();
        assert_eq!(allowed_count, 2);
    }

    #[test]
    fn to_cli_args_with_extra_args_boolean_flag() {
        let config = ClientConfig::builder()
            .prompt("test")
            .extra_args(BTreeMap::from([("replay-user-messages".into(), None)]))
            .build();
        let args = config.to_cli_args();
        assert!(args.contains(&"--replay-user-messages".into()));
    }

    #[test]
    fn to_cli_args_with_extra_args_valued_flag() {
        let config = ClientConfig::builder()
            .prompt("test")
            .extra_args(BTreeMap::from([(
                "context-window".into(),
                Some("200000".into()),
            )]))
            .build();
        let args = config.to_cli_args();
        let idx = args.iter().position(|a| a == "--context-window").unwrap();
        assert_eq!(args[idx + 1], "200000");
    }

    #[test]
    fn builder_timeout_defaults() {
        let config = ClientConfig::builder().prompt("test").build();
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(30)));
        assert_eq!(config.close_timeout, Some(Duration::from_secs(10)));
        assert_eq!(config.read_timeout, None);
        assert_eq!(config.default_hook_timeout, Duration::from_secs(30));
        assert_eq!(config.version_check_timeout, Some(Duration::from_secs(5)));
    }

    #[test]
    fn builder_custom_timeouts() {
        let config = ClientConfig::builder()
            .prompt("test")
            .connect_timeout(Some(Duration::from_secs(60)))
            .close_timeout(Some(Duration::from_secs(20)))
            .read_timeout(Some(Duration::from_secs(120)))
            .default_hook_timeout(Duration::from_secs(10))
            .version_check_timeout(Some(Duration::from_secs(15)))
            .build();
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(60)));
        assert_eq!(config.close_timeout, Some(Duration::from_secs(20)));
        assert_eq!(config.read_timeout, Some(Duration::from_secs(120)));
        assert_eq!(config.default_hook_timeout, Duration::from_secs(10));
        assert_eq!(config.version_check_timeout, Some(Duration::from_secs(15)));
    }

    #[test]
    fn builder_disable_connect_timeout() {
        let config = ClientConfig::builder()
            .prompt("test")
            .connect_timeout(None::<Duration>)
            .build();
        assert_eq!(config.connect_timeout, None);
    }

    #[test]
    fn builder_cancellation_token() {
        let token = CancellationToken::new();
        let config = ClientConfig::builder()
            .prompt("test")
            .cancellation_token(token.clone())
            .build();
        assert!(config.cancellation_token.is_some());
    }

    #[test]
    fn builder_cancellation_token_default_is_none() {
        let config = ClientConfig::builder().prompt("test").build();
        assert!(config.cancellation_token.is_none());
    }

    #[test]
    fn to_cli_args_with_resume() {
        let config = ClientConfig::builder()
            .prompt("test")
            .resume("session-123")
            .build();
        let args = config.to_cli_args();
        assert!(args.contains(&"--resume".into()));
        assert!(args.contains(&"session-123".into()));
    }
}
