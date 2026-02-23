//! MCP (Model Context Protocol) server configuration types.
//!
//! These types define how MCP servers are configured for use with
//! Claude Code sessions. They are passed via [`ClientConfig`](crate::config::ClientConfig)
//! and serialized into the CLI invocation.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A map of MCP server names to their configurations.
pub type McpServers = HashMap<String, McpServerConfig>;

/// Configuration for a single MCP server that the CLI should connect to.
///
/// This maps to the `--mcp-servers` JSON argument passed to the CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// The command to start the MCP server (e.g., `"npx"`, `"uvx"`).
    pub command: String,

    /// Arguments passed to the command.
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables to set for the MCP server process.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Working directory for the MCP server process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

impl McpServerConfig {
    /// Create a new MCP server config with just a command.
    #[must_use]
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            env: HashMap::new(),
            cwd: None,
        }
    }

    /// Add arguments to the server command.
    #[must_use]
    pub fn with_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    /// Add environment variables for the server process.
    #[must_use]
    pub fn with_env(
        mut self,
        env: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        self.env = env.into_iter().map(|(k, v)| (k.into(), v.into())).collect();
        self
    }

    /// Set the working directory for the server process.
    #[must_use]
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_server_config_round_trip() {
        let config = McpServerConfig {
            command: "npx".into(),
            args: vec![
                "-y".into(),
                "@modelcontextprotocol/server-filesystem".into(),
            ],
            env: HashMap::from([("HOME".into(), "/home/user".into())]),
            cwd: Some(PathBuf::from("/workspace")),
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: McpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn mcp_server_config_minimal() {
        let json = r#"{"command":"npx"}"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.command, "npx");
        assert!(config.args.is_empty());
        assert!(config.env.is_empty());
        assert!(config.cwd.is_none());
    }

    #[test]
    fn mcp_server_config_builder_pattern() {
        let config = McpServerConfig::new("uvx")
            .with_args(["mcp-server-git"])
            .with_env([("GIT_DIR", "/repo/.git")])
            .with_cwd("/repo");

        assert_eq!(config.command, "uvx");
        assert_eq!(config.args, ["mcp-server-git"]);
        assert_eq!(config.env["GIT_DIR"], "/repo/.git");
        assert_eq!(config.cwd, Some(PathBuf::from("/repo")));
    }

    #[test]
    fn mcp_servers_map() {
        let mut servers = McpServers::new();
        servers.insert(
            "filesystem".into(),
            McpServerConfig::new("npx")
                .with_args(["-y", "@modelcontextprotocol/server-filesystem"]),
        );
        servers.insert(
            "git".into(),
            McpServerConfig::new("uvx").with_args(["mcp-server-git"]),
        );

        let json = serde_json::to_string(&servers).unwrap();
        let decoded: McpServers = serde_json::from_str(&json).unwrap();
        assert_eq!(servers, decoded);
    }

    #[test]
    fn mcp_server_status_from_cli_output() {
        // Re-export test — McpServerStatus lives in types::messages
        let json = r#"{"name":"my-server","status":"connected"}"#;
        let status: crate::types::messages::McpServerStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.name, "my-server");
        assert_eq!(status.status, "connected");
    }
}
