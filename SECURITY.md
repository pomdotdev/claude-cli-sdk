# Security Policy

## Scope

`claude-cli-sdk` is a local SDK that wraps the Claude Code CLI process via stdio. It does not make network connections itself — all network access is managed by the underlying CLI binary.

The SDK handles:
- Process spawning and lifecycle management
- NDJSON parsing of CLI output
- Permission callback dispatch
- Hook callback dispatch with timeout enforcement

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. **Preferred**: Open a [GitHub Security Advisory](https://github.com/pomdotdev/claude-cli-sdk/security/advisories/new)
2. **Alternative**: Email the maintainers directly (see repository contacts)

Please do **not** open a public issue for security vulnerabilities.

### What to include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### Response timeline

- **Acknowledgment**: Within 48 hours
- **Initial assessment**: Within 1 week
- **Fix and disclosure**: Coordinated with reporter

## Security Considerations

- The SDK spawns child processes — ensure `cli_path` is trusted
- Hook callbacks run with the SDK's process permissions
- `BypassPermissions` mode disables all tool approval checks — use only in trusted CI environments
- Environment variables passed via `ClientConfig::env` are forwarded to the child process
