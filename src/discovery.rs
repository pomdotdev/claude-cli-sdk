//! CLI discovery — locating the Claude Code binary on the system.
//!
//! The SDK requires the Claude Code CLI to be installed on the host.  This
//! module searches a priority-ordered list of platform-specific paths and
//! optionally verifies the installed version against [`MIN_CLI_VERSION`].
//!
//! On all platforms, `which::which("claude")` is tried first (`$PATH` lookup).
//! Fallback paths are platform-specific:
//!
//! - **Unix** (macOS/Linux): `~/.npm-global/bin/claude`, `/usr/local/bin/claude`, etc.
//! - **Windows**: `AppData/Roaming/npm/claude.exe`, `scoop/shims/claude.exe`, etc.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::time::Duration;
//! use claude_cli_sdk::discovery::{find_cli, check_cli_version};
//!
//! # async fn example() -> claude_cli_sdk::Result<()> {
//! let cli = find_cli()?;
//! let version = check_cli_version(&cli, Some(Duration::from_secs(5))).await?;
//! println!("Found Claude CLI v{version} at {}", cli.display());
//! # Ok(())
//! # }
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::errors::{Error, Result};

/// Minimum CLI version required by this SDK.
pub const MIN_CLI_VERSION: &str = "1.0.0";

/// Binary name to search for.
const CLI_NAME: &str = "claude";

/// Locate the Claude Code CLI binary, searching a priority-ordered list of
/// platform-specific paths.
///
/// # Discovery Strategy (priority order)
///
/// 1. `which claude` — whatever is first on `$PATH` (cross-platform)
///
/// **Unix fallbacks:**
/// 2. `~/.npm-global/bin/claude`
/// 3. `/usr/local/bin/claude`
/// 4. `~/.local/bin/claude`
/// 5. `node_modules/.bin/claude` (relative to CWD)
/// 6. `~/.yarn/bin/claude`
/// 7. `~/.claude/local/claude`
///
/// **Windows fallbacks:**
/// 2. `%USERPROFILE%/AppData/Roaming/npm/claude.exe`
/// 3. `node_modules/.bin/claude.exe` (relative to CWD)
/// 4. `%USERPROFILE%/scoop/shims/claude.exe`
/// 5. `%USERPROFILE%/.claude/local/claude.exe`
///
/// # Errors
///
/// Returns [`Error::CliNotFound`] if no binary is found at any location.
pub fn find_cli() -> Result<PathBuf> {
    // 1. which — respects $PATH, handles .exe / PATHEXT on Windows.
    if let Ok(path) = which::which(CLI_NAME) {
        return Ok(path);
    }

    // Platform-specific fallback paths.
    let home = home_dir();

    #[cfg(unix)]
    let candidates: Vec<PathBuf> = vec![
        home.as_ref()
            .map(|h| h.join(".npm-global/bin").join(CLI_NAME)),
        Some(PathBuf::from("/usr/local/bin").join(CLI_NAME)),
        home.as_ref().map(|h| h.join(".local/bin").join(CLI_NAME)),
        Some(PathBuf::from("node_modules/.bin").join(CLI_NAME)),
        home.as_ref().map(|h| h.join(".yarn/bin").join(CLI_NAME)),
        home.as_ref()
            .map(|h| h.join(".claude/local").join(CLI_NAME)),
    ]
    .into_iter()
    .flatten()
    .collect();

    #[cfg(windows)]
    let cli_exe = format!("{CLI_NAME}.exe");
    #[cfg(windows)]
    let candidates: Vec<PathBuf> = vec![
        home.as_ref()
            .map(|h| h.join("AppData/Roaming/npm").join(&cli_exe)),
        Some(PathBuf::from("node_modules/.bin").join(&cli_exe)),
        home.as_ref().map(|h| h.join("scoop/shims").join(&cli_exe)),
        home.as_ref()
            .map(|h| h.join(".claude/local").join(&cli_exe)),
    ]
    .into_iter()
    .flatten()
    .collect();

    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(Error::CliNotFound)
}

/// Run `claude --version` and parse the semver string from the output.
///
/// The CLI typically prints a line like `claude v1.2.3` or just `1.2.3`.
/// This function extracts the first semver-like substring (digits and dots).
///
/// If `timeout` is `Some`, the command is killed after the deadline expires.
///
/// # Errors
///
/// - [`Error::Timeout`] if the deadline expires.
/// - [`Error::SpawnFailed`] if the process cannot be launched.
/// - [`Error::ProcessExited`] if the process exits with a non-zero code.
/// - [`Error::ControlProtocol`] if the output cannot be parsed as a version.
pub async fn check_cli_version(cli_path: &Path, timeout: Option<Duration>) -> Result<String> {
    let mut child = tokio::process::Command::new(cli_path)
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(Error::SpawnFailed)?;

    if let Some(d) = timeout {
        if tokio::time::timeout(d, child.wait()).await.is_err() {
            let _ = child.kill().await;
            return Err(Error::Timeout(format!(
                "version check timed out after {}s",
                d.as_secs_f64()
            )));
        }
    }

    let output = child.wait_with_output().await.map_err(Error::SpawnFailed)?;

    if !output.status.success() {
        return Err(Error::ProcessExited {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_version(&stdout).ok_or_else(|| {
        Error::ControlProtocol(format!("could not parse version from CLI output: {stdout}"))
    })
}

/// Compare two semver-like version strings (major.minor.patch).
///
/// Returns `true` if `version >= minimum`.
#[must_use]
pub fn version_satisfies(version: &str, minimum: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let mut parts = s.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        let major = parts.next().unwrap_or(0);
        let minor = parts.next().unwrap_or(0);
        let patch = parts.next().unwrap_or(0);
        (major, minor, patch)
    };
    parse(version) >= parse(minimum)
}

// ── Internals ────────────────────────────────────────────────────────────────

/// Extract a semver-like version string from CLI output.
///
/// Handles formats like `claude v1.2.3`, `1.2.3`, `v1.2.3-beta.1`.
fn parse_version(output: &str) -> Option<String> {
    // Look for a pattern like digits.digits.digits (possibly with pre-release suffix).
    for word in output.split_whitespace() {
        let trimmed = word.strip_prefix('v').unwrap_or(word);
        if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) && trimmed.contains('.') {
            // Take up to the first character that isn't digit, dot, or hyphen/alpha (for pre-release).
            let version: String = trimmed
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
                .collect();
            if version.split('.').count() >= 2 {
                return Some(version);
            }
        }
    }
    None
}

/// Get the user's home directory.
///
/// - **Unix**: reads `$HOME`.
/// - **Windows**: reads `$USERPROFILE`, falling back to `$HOMEDRIVE` + `$HOMEPATH`.
fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| {
                let drive = std::env::var_os("HOMEDRIVE")?;
                let path = std::env::var_os("HOMEPATH")?;
                Some(PathBuf::from(drive).join(path))
            })
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_standard() {
        assert_eq!(parse_version("1.2.3"), Some("1.2.3".into()));
    }

    #[test]
    fn parse_version_with_v_prefix() {
        assert_eq!(parse_version("v1.2.3"), Some("1.2.3".into()));
    }

    #[test]
    fn parse_version_with_claude_prefix() {
        assert_eq!(parse_version("claude v1.0.12"), Some("1.0.12".into()));
    }

    #[test]
    fn parse_version_prerelease() {
        assert_eq!(parse_version("v2.0.0-beta.1"), Some("2.0.0-beta.1".into()));
    }

    #[test]
    fn parse_version_empty_input() {
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn parse_version_no_version() {
        assert_eq!(parse_version("no version here"), None);
    }

    #[test]
    fn version_satisfies_equal() {
        assert!(version_satisfies("1.0.0", "1.0.0"));
    }

    #[test]
    fn version_satisfies_greater() {
        assert!(version_satisfies("2.0.0", "1.0.0"));
        assert!(version_satisfies("1.1.0", "1.0.0"));
        assert!(version_satisfies("1.0.1", "1.0.0"));
    }

    #[test]
    fn version_satisfies_less() {
        assert!(!version_satisfies("0.9.0", "1.0.0"));
        assert!(!version_satisfies("1.0.0", "1.0.1"));
    }

    #[test]
    fn version_satisfies_major_priority() {
        assert!(version_satisfies("2.0.0", "1.9.9"));
        assert!(!version_satisfies("1.9.9", "2.0.0"));
    }

    #[test]
    fn find_cli_returns_path_or_not_found() {
        // This test is environment-dependent. We just verify the function doesn't panic.
        let result = find_cli();
        match result {
            Ok(path) => assert!(path.is_file()),
            Err(Error::CliNotFound) => {} // Expected when CLI not installed
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    #[tokio::test]
    async fn check_cli_version_with_timeout() {
        // Create a script/batch file that ignores --version and blocks.
        let dir = tempfile::tempdir().unwrap();

        #[cfg(unix)]
        let script = {
            let s = dir.path().join("slow_cli");
            std::fs::write(&s, "#!/bin/sh\nsleep 999\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&s, std::fs::Permissions::from_mode(0o755)).unwrap();
            s
        };

        #[cfg(windows)]
        let script = {
            let s = dir.path().join("slow_cli.bat");
            std::fs::write(&s, "@ping -n 999 127.0.0.1 >nul\r\n").unwrap();
            s
        };

        let result = check_cli_version(&script, Some(Duration::from_millis(50))).await;
        assert!(
            matches!(result, Err(Error::Timeout(_))),
            "expected Timeout, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn check_cli_version_no_timeout() {
        // With None timeout, a fast command completes normally.
        // We need a binary that exits 0 but outputs no parseable version.
        // Note: /usr/bin/true on GNU coreutils responds to --version with
        // version info, so we create a custom no-op script instead.
        let dir = tempfile::tempdir().unwrap();

        #[cfg(unix)]
        let bin = {
            use std::os::unix::fs::PermissionsExt;
            let s = dir.path().join("noop.sh");
            std::fs::write(&s, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(&s, std::fs::Permissions::from_mode(0o755)).unwrap();
            s
        };

        #[cfg(windows)]
        let bin = {
            let s = dir.path().join("noop.bat");
            std::fs::write(&s, "@exit /b 0\r\n").unwrap();
            s
        };

        let result = check_cli_version(bin.as_ref(), None).await;
        assert!(
            matches!(result, Err(Error::ControlProtocol(_))),
            "expected ControlProtocol (no version), got: {result:?}"
        );
    }
}
