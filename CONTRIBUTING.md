# Contributing to claude-cli-sdk

Thank you for your interest in contributing! This document provides guidelines for contributing to the project.

## Prerequisites

- **Rust 1.85+** (edition 2024)
- **macOS or Linux** (Windows is not supported)
- **Claude Code CLI** installed for integration tests

## Getting Started

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/<your-username>/claude-cli-sdk.git
   cd claude-cli-sdk
   ```
3. Create a feature branch:
   ```bash
   git checkout -b feat/your-feature
   ```

## Building and Testing

```bash
# Build
cargo build

# Run unit tests
cargo test

# Run tests with mock transport support
cargo test --features testing

# Run integration tests (requires a live Claude CLI)
cargo test --features integration

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt --check

# Build documentation
cargo doc --no-deps
```

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy -- -D warnings` and fix all warnings
- Follow Rust naming conventions (RFC 430)
- Add doc comments to all public items
- Include unit tests for new functionality

## Pull Request Process

1. Ensure all tests pass (`cargo test --features testing`)
2. Ensure no clippy warnings (`cargo clippy -- -D warnings`)
3. Update `CHANGELOG.md` under `[Unreleased]`
4. Update documentation if you changed public API
5. Open a PR with a clear description of the changes
6. Wait for review — maintainers may request changes

## Reporting Issues

When filing a bug report, please include:

- **SDK version** (`claude-cli-sdk` version from `Cargo.toml`)
- **CLI version** (`claude --version`)
- **OS** (macOS version or Linux distribution)
- **Rust version** (`rustc --version`)
- **Minimal reproduction** — the smallest code that reproduces the issue
- **Expected vs actual behavior**

## License

By contributing, you agree that your contributions will be licensed under the same dual license as the project: [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE).
