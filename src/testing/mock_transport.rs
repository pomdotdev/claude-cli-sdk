//! In-memory mock transport for deterministic SDK testing.

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::errors::{Error, Result};
use crate::transport::Transport;

/// An in-memory, synchronised mock transport for deterministic SDK testing.
///
/// Pre-load NDJSON lines via [`enqueue`](Self::enqueue), then use through the
/// [`Transport`] trait. Inspect what the SDK "wrote" via
/// [`written_lines`](Self::written_lines).
pub struct MockTransport {
    messages: Mutex<VecDeque<String>>,
    written: Mutex<Vec<String>>,
    ready: AtomicBool,
    /// Artificial delay applied during [`connect()`](Transport::connect).
    connect_delay: Mutex<Option<Duration>>,
    /// Artificial delay applied before each message in [`read_messages()`](Transport::read_messages).
    recv_delay: Mutex<Option<Duration>>,
}

impl std::fmt::Debug for MockTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let messages = self.messages.lock().unwrap();
        let written = self.written.lock().unwrap();
        f.debug_struct("MockTransport")
            .field("queued", &messages.len())
            .field("written", &written.len())
            .field("ready", &self.ready.load(Ordering::Relaxed))
            .finish()
    }
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl MockTransport {
    /// Create an empty [`MockTransport`] with no pre-loaded messages.
    #[must_use]
    pub fn new() -> Self {
        Self {
            messages: Mutex::new(VecDeque::new()),
            written: Mutex::new(Vec::new()),
            ready: AtomicBool::new(false),
            connect_delay: Mutex::new(None),
            recv_delay: Mutex::new(None),
        }
    }

    /// Set an artificial delay for [`connect()`](Transport::connect).
    ///
    /// Useful for testing connect timeout behavior.
    pub fn set_connect_delay(&self, delay: Duration) {
        *self.connect_delay.lock().expect("lock") = Some(delay);
    }

    /// Set an artificial delay applied before each message in
    /// [`read_messages()`](Transport::read_messages).
    ///
    /// Useful for testing read timeout behavior.
    pub fn set_recv_delay(&self, delay: Duration) {
        *self.recv_delay.lock().expect("lock") = Some(delay);
    }

    /// Enqueue a raw JSON line that the SDK will receive as if it came from
    /// the Claude CLI stdout.
    pub fn enqueue(&self, json: &str) {
        self.messages
            .lock()
            .expect("lock")
            .push_back(json.to_owned());
    }

    /// Enqueue a serializable value as JSON.
    pub fn enqueue_value(&self, value: &impl serde::Serialize) {
        let json = serde_json::to_string(value).expect("serialize");
        self.enqueue(&json);
    }

    /// Return all lines that have been written to the transport.
    #[must_use]
    pub fn written_lines(&self) -> Vec<String> {
        self.written.lock().expect("lock").clone()
    }

    /// Return the number of messages still queued.
    #[must_use]
    pub fn queued_count(&self) -> usize {
        self.messages.lock().expect("lock").len()
    }

    /// Return the number of lines written.
    #[must_use]
    pub fn written_count(&self) -> usize {
        self.written.lock().expect("lock").len()
    }

    /// Clear all state.
    pub fn reset(&self) {
        self.messages.lock().expect("lock").clear();
        self.written.lock().expect("lock").clear();
        *self.connect_delay.lock().expect("lock") = None;
        *self.recv_delay.lock().expect("lock") = None;
        self.ready.store(false, Ordering::Release);
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn connect(&self) -> Result<()> {
        let delay = *self.connect_delay.lock().expect("lock");
        if let Some(d) = delay {
            tokio::time::sleep(d).await;
        }
        self.ready.store(true, Ordering::Release);
        Ok(())
    }

    async fn write(&self, data: &str) -> Result<()> {
        if !self.is_ready() {
            return Err(Error::NotConnected);
        }
        self.written.lock().expect("lock").push(data.to_owned());
        Ok(())
    }

    fn read_messages(&self) -> Pin<Box<dyn Stream<Item = Result<serde_json::Value>> + Send>> {
        // Drain all queued messages into a channel.
        let messages: Vec<String> = {
            let mut guard = self.messages.lock().expect("lock");
            guard.drain(..).collect()
        };

        let recv_delay = *self.recv_delay.lock().expect("lock");
        let (tx, rx) = mpsc::channel(messages.len().max(1));

        tokio::spawn(async move {
            for line in messages {
                if let Some(delay) = recv_delay {
                    tokio::time::sleep(delay).await;
                }
                let result = serde_json::from_str::<serde_json::Value>(&line).map_err(|e| {
                    Error::ParseError {
                        message: e.to_string(),
                        line: line.clone(),
                    }
                });
                if tx.send(result).await.is_err() {
                    break;
                }
            }
        });

        Box::pin(ReceiverStream::new(rx))
    }

    async fn end_input(&self) -> Result<()> {
        Ok(())
    }

    async fn interrupt(&self) -> Result<()> {
        Ok(())
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    async fn close(&self) -> Result<Option<i32>> {
        self.ready.store(false, Ordering::Release);
        Ok(Some(0))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[test]
    fn new_transport_is_empty() {
        let t = MockTransport::new();
        assert_eq!(t.queued_count(), 0);
        assert_eq!(t.written_count(), 0);
    }

    #[test]
    fn default_is_same_as_new() {
        let t = MockTransport::default();
        assert_eq!(t.queued_count(), 0);
    }

    #[test]
    fn enqueue_increments_count() {
        let t = MockTransport::new();
        t.enqueue(r#"{"type":"system"}"#);
        assert_eq!(t.queued_count(), 1);
    }

    #[tokio::test]
    async fn connect_sets_ready() {
        let t = MockTransport::new();
        assert!(!t.is_ready());
        t.connect().await.unwrap();
        assert!(t.is_ready());
    }

    #[tokio::test]
    async fn write_fails_when_not_connected() {
        let t = MockTransport::new();
        let err = t.write("test").await.unwrap_err();
        assert!(matches!(err, Error::NotConnected));
    }

    #[tokio::test]
    async fn write_records_lines() {
        let t = MockTransport::new();
        t.connect().await.unwrap();
        t.write("line1").await.unwrap();
        t.write("line2").await.unwrap();
        assert_eq!(t.written_lines(), vec!["line1", "line2"]);
    }

    #[tokio::test]
    async fn read_messages_yields_parsed_json() {
        let t = MockTransport::new();
        t.enqueue(r#"{"type":"system","subtype":"init","session_id":"s1","cwd":"/","tools":[],"mcp_servers":[],"model":"m"}"#);
        t.enqueue(
            r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"usage":{}}"#,
        );
        t.connect().await.unwrap();

        let mut stream = t.read_messages();
        let mut values = Vec::new();
        while let Some(item) = stream.next().await {
            values.push(item.unwrap());
        }
        assert_eq!(values.len(), 2);
        assert_eq!(values[0]["type"], "system");
        assert_eq!(values[1]["type"], "result");
    }

    #[tokio::test]
    async fn close_clears_ready() {
        let t = MockTransport::new();
        t.connect().await.unwrap();
        assert!(t.is_ready());
        t.close().await.unwrap();
        assert!(!t.is_ready());
    }

    #[test]
    fn reset_clears_all() {
        let t = MockTransport::new();
        t.enqueue("a");
        t.reset();
        assert_eq!(t.queued_count(), 0);
        assert_eq!(t.written_count(), 0);
    }

    #[test]
    fn debug_does_not_panic() {
        let t = MockTransport::new();
        let _ = format!("{t:?}");
    }
}
