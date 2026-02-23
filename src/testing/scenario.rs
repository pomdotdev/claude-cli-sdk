//! Scenario builder for constructing multi-turn test scenarios.

use crate::testing::mock_transport::MockTransport;
use crate::types::messages::Message;

/// Fluent builder for constructing multi-turn test scenarios.
///
/// # Example
///
/// ```rust,no_run
/// use claude_cli_sdk::testing::{ScenarioBuilder, system_init, assistant_text, result_success};
///
/// let transport = ScenarioBuilder::new("session-1")
///     .exchange(vec![assistant_text("Hello!")])
///     .build();
/// ```
pub struct ScenarioBuilder {
    session_id: String,
    exchanges: Vec<Vec<Message>>,
}

impl ScenarioBuilder {
    /// Create a new scenario with the given session ID.
    #[must_use]
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            exchanges: Vec::new(),
        }
    }

    /// Add an exchange (a sequence of messages the CLI produces for one turn).
    #[must_use]
    pub fn exchange(mut self, messages: Vec<Message>) -> Self {
        self.exchanges.push(messages);
        self
    }

    /// Build a [`MockTransport`] pre-loaded with the scenario messages.
    ///
    /// The transport is loaded with:
    /// 1. A system/init message with the scenario's session ID
    /// 2. All exchange messages in order
    /// 3. A result/success message
    #[must_use]
    pub fn build(self) -> MockTransport {
        let transport = MockTransport::new();

        // System init
        let init = crate::testing::builders::system_init(&self.session_id);
        transport.enqueue_value(&init);

        // Exchange messages
        for exchange in &self.exchanges {
            for msg in exchange {
                transport.enqueue_value(msg);
            }
        }

        // Result
        let result = crate::testing::builders::result_success(&self.session_id);
        transport.enqueue_value(&result);

        transport
    }

    /// Build a [`MockTransport`] without the auto-generated init/result bookends.
    ///
    /// Use this when you want full control over the message sequence.
    #[must_use]
    pub fn build_raw(self) -> MockTransport {
        let transport = MockTransport::new();
        for exchange in &self.exchanges {
            for msg in exchange {
                transport.enqueue_value(msg);
            }
        }
        transport
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::builders::*;

    #[test]
    fn scenario_builder_creates_transport_with_bookends() {
        let transport = ScenarioBuilder::new("s1")
            .exchange(vec![assistant_text("Hello!")])
            .build();

        // init + assistant + result = 3 messages
        assert_eq!(transport.queued_count(), 3);
    }

    #[test]
    fn scenario_builder_empty_exchange() {
        let transport = ScenarioBuilder::new("s1").build();

        // init + result = 2 messages
        assert_eq!(transport.queued_count(), 2);
    }

    #[test]
    fn scenario_builder_multiple_exchanges() {
        let transport = ScenarioBuilder::new("s1")
            .exchange(vec![assistant_text("Turn 1")])
            .exchange(vec![assistant_text("Turn 2")])
            .build();

        // init + 2 assistants + result = 4
        assert_eq!(transport.queued_count(), 4);
    }

    #[test]
    fn scenario_builder_raw() {
        let transport = ScenarioBuilder::new("s1")
            .exchange(vec![assistant_text("raw")])
            .build_raw();

        // Only the exchange, no bookends
        assert_eq!(transport.queued_count(), 1);
    }
}
