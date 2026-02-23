//! Error types for the claude-cli-sdk.
//!
//! All fallible operations in this crate return [`Result<T>`], which is an alias
//! for `std::result::Result<T, Error>`.
//!
//! # Example
//!
//! ```rust
//! use claude_cli_sdk::{Error, Result};
//!
//! fn might_fail() -> Result<()> {
//!     Err(Error::NotConnected)
//! }
//! ```

/// All errors that can be produced by the claude-code SDK.
///
/// Variants are non-exhaustive in the sense that future SDK versions may add
/// new variants — callers should include a wildcard arm when matching.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The Claude Code CLI binary was not found on `PATH`.
    ///
    /// Install it with: `npm install -g @anthropic-ai/claude-code`
    #[error("Claude Code CLI not found. Install: npm install -g @anthropic-ai/claude-code")]
    CliNotFound,

    /// The discovered CLI version is below the minimum required by this SDK.
    #[error("CLI version {found} below minimum {required}")]
    VersionMismatch {
        /// The version that was discovered on the system.
        found: String,
        /// The minimum version required by this SDK.
        required: String,
    },

    /// The OS failed to spawn the Claude child process.
    #[error("Failed to spawn Claude process: {0}")]
    SpawnFailed(#[source] std::io::Error),

    /// The Claude process exited with a non-zero (or missing) status code.
    ///
    /// `code` is `None` when the process was killed by a signal.
    #[error("Claude process exited with code {code:?}: {stderr}")]
    ProcessExited {
        /// The exit code, or `None` if the process was killed by a signal.
        code: Option<i32>,
        /// Captured stderr output from the process.
        stderr: String,
    },

    /// A line from the Claude process could not be parsed as valid JSON.
    #[error("Failed to parse JSON: {message} (line: {line})")]
    ParseError {
        /// Human-readable description of the parse failure.
        message: String,
        /// The raw line that could not be parsed (truncated to 200 chars).
        line: String,
    },

    /// Transparent wrapper around [`std::io::Error`].
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Transparent wrapper around [`serde_json::Error`].
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// A method that requires an active connection was called before [`connect()`](crate::Client).
    #[error("Client not connected. Call connect() first.")]
    NotConnected,

    /// An error in the underlying stdio/socket transport layer.
    #[error("Transport error: {0}")]
    Transport(String),

    /// A configuration value is absent or out of range.
    #[error("Invalid configuration: {0}")]
    Config(String),

    /// The structured control protocol exchanged with the CLI returned an
    /// unexpected message or sequence.
    #[error("Control protocol error: {0}")]
    ControlProtocol(String),

    /// An image payload failed validation (unsupported MIME type or exceeds
    /// the 15 MiB base64 size limit).
    #[error("Image validation error: {0}")]
    ImageValidation(String),

    /// An async operation exceeded its deadline.
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// The operation was cancelled via a [`CancellationToken`](tokio_util::sync::CancellationToken).
    #[error("Operation cancelled")]
    Cancelled,
}

/// Convenience alias so callers can write `Result<T>` instead of
/// `std::result::Result<T, claude_cli_sdk::Error>`.
pub type Result<T> = std::result::Result<T, Error>;

// ── Helpers ──────────────────────────────────────────────────────────────────

impl Error {
    /// Returns `true` if this error indicates the process exited cleanly but
    /// with a failure code (i.e., not a transport or protocol fault).
    #[inline]
    #[must_use]
    pub fn is_process_exit(&self) -> bool {
        matches!(self, Self::ProcessExited { .. })
    }

    /// Returns `true` if this error is transient and the caller might
    /// reasonably retry (e.g., I/O errors, timeouts).
    #[inline]
    #[must_use]
    pub fn is_retriable(&self) -> bool {
        matches!(self, Self::Io(_) | Self::Timeout(_) | Self::Transport(_))
    }

    /// Returns `true` if this error indicates the operation was cancelled
    /// via a [`CancellationToken`](tokio_util::sync::CancellationToken).
    #[inline]
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: convert an Error to its Display string.
    fn display(e: &Error) -> String {
        e.to_string()
    }

    #[test]
    fn cli_not_found_display() {
        let e = Error::CliNotFound;
        assert!(
            display(&e).contains("npm install -g @anthropic-ai/claude-code"),
            "message should include install hint, got: {e}"
        );
    }

    #[test]
    fn version_mismatch_display() {
        let e = Error::VersionMismatch {
            found: "1.0.0".to_owned(),
            required: "2.0.0".to_owned(),
        };
        let s = display(&e);
        assert!(s.contains("1.0.0"), "should contain found version");
        assert!(s.contains("2.0.0"), "should contain required version");
    }

    #[test]
    fn spawn_failed_display() {
        let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let e = Error::SpawnFailed(inner);
        assert!(display(&e).contains("Failed to spawn Claude process"));
    }

    #[test]
    fn process_exited_display_with_code() {
        let e = Error::ProcessExited {
            code: Some(1),
            stderr: "fatal error".to_owned(),
        };
        let s = display(&e);
        assert!(s.contains("1"), "should contain exit code");
        assert!(s.contains("fatal error"), "should contain stderr");
    }

    #[test]
    fn process_exited_display_signal_kill() {
        let e = Error::ProcessExited {
            code: None,
            stderr: String::new(),
        };
        // code: None should render as "None"
        assert!(display(&e).contains("None"));
    }

    #[test]
    fn parse_error_display() {
        let e = Error::ParseError {
            message: "unexpected token".to_owned(),
            line: "{bad json}".to_owned(),
        };
        let s = display(&e);
        assert!(s.contains("unexpected token"));
        assert!(s.contains("{bad json}"));
    }

    #[test]
    fn io_error_display() {
        let inner = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
        let e = Error::from(inner);
        assert!(display(&e).contains("I/O error"));
    }

    #[test]
    fn json_error_display() {
        let inner: serde_json::Error =
            serde_json::from_str::<serde_json::Value>("{bad}").unwrap_err();
        let e = Error::from(inner);
        assert!(display(&e).contains("JSON error"));
    }

    #[test]
    fn not_connected_display() {
        let e = Error::NotConnected;
        assert!(display(&e).contains("connect()"));
    }

    #[test]
    fn transport_display() {
        let e = Error::Transport("connection refused".to_owned());
        assert!(display(&e).contains("connection refused"));
    }

    #[test]
    fn config_display() {
        let e = Error::Config("max_turns must be > 0".to_owned());
        assert!(display(&e).contains("max_turns must be > 0"));
    }

    #[test]
    fn control_protocol_display() {
        let e = Error::ControlProtocol("unexpected init sequence".to_owned());
        assert!(display(&e).contains("unexpected init sequence"));
    }

    #[test]
    fn image_validation_display() {
        let e = Error::ImageValidation("unsupported MIME type: image/bmp".to_owned());
        assert!(display(&e).contains("image/bmp"));
    }

    #[test]
    fn timeout_display() {
        let e = Error::Timeout("connect timed out after 30s".to_owned());
        assert!(display(&e).contains("30s"));
    }

    // ── Helper methods ────────────────────────────────────────────────────────

    #[test]
    fn is_process_exit_true_for_process_exited() {
        let e = Error::ProcessExited {
            code: Some(1),
            stderr: String::new(),
        };
        assert!(e.is_process_exit());
    }

    #[test]
    fn is_process_exit_false_for_other_variants() {
        assert!(!Error::NotConnected.is_process_exit());
        assert!(!Error::CliNotFound.is_process_exit());
    }

    #[test]
    fn is_retriable_for_io_timeout_transport() {
        let io_err = Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "reset",
        ));
        assert!(io_err.is_retriable());
        assert!(Error::Timeout("deadline".to_owned()).is_retriable());
        assert!(Error::Transport("broken".to_owned()).is_retriable());
    }

    #[test]
    fn is_retriable_false_for_not_connected() {
        assert!(!Error::NotConnected.is_retriable());
    }

    #[test]
    fn cancelled_display() {
        let e = Error::Cancelled;
        assert!(display(&e).contains("cancelled"));
    }

    #[test]
    fn is_cancelled_true_for_cancelled() {
        assert!(Error::Cancelled.is_cancelled());
    }

    #[test]
    fn is_cancelled_false_for_other_variants() {
        assert!(!Error::NotConnected.is_cancelled());
        assert!(!Error::CliNotFound.is_cancelled());
        assert!(!Error::Timeout("x".into()).is_cancelled());
    }

    #[test]
    fn cancelled_is_not_retriable() {
        assert!(!Error::Cancelled.is_retriable());
    }

    #[test]
    fn result_alias_compiles() {
        fn ok_fn() -> Result<u32> {
            Ok(42)
        }
        assert_eq!(ok_fn().unwrap(), 42);
    }
}
