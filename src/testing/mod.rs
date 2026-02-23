//! Testing utilities (behind the `testing` feature flag).
//!
//! Enable with:
//!
//! ```toml
//! [dev-dependencies]
//! claude-cli-sdk = { version = "*", features = ["testing"] }
//! ```
//!
//! # Components
//!
//! - [`MockTransport`] — In-memory transport implementing [`Transport`](crate::transport::Transport).
//! - [`ScenarioBuilder`] — Fluent API for constructing multi-turn test scenarios.
//! - [`builders`] — Convenience constructors for message types.

pub mod builders;
pub mod mock_transport;
pub mod scenario;

pub use builders::*;
pub use mock_transport::MockTransport;
pub use scenario::ScenarioBuilder;
