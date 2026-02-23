//! The core `Client` struct — multi-turn, bidirectional Claude Code sessions.
//!
//! # Architecture
//!
//! On [`connect()`](Client::connect), the client spawns a background task that:
//! 1. Reads JSON values from the transport
//! 2. Routes permission requests to the configured callback
//! 3. Routes hook requests to registered hook matchers
//! 4. Applies the message callback (if any)
//! 5. Forwards resulting messages through a `flume` channel
//!
//! Callers consume messages via [`send()`](Client::send) (which returns a
//! stream), or via [`receive_messages()`](Client::receive_messages).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use futures_core::Stream;
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::callback::apply_callback;
use crate::config::{ClientConfig, PermissionMode};
use crate::errors::{Error, Result};
use crate::transport::{CliTransport, Transport};
use crate::types::content::UserContent;
use crate::types::messages::{Message, SessionInfo};

// ── Cancellation helper ──────────────────────────────────────────────────────

/// Wait for the token to fire, or pend forever if `None`.
///
/// Useful as a `tokio::select!` branch that compiles away when no token is
/// provided.
pub(crate) async fn cancelled_or_pending(token: Option<&CancellationToken>) {
    match token {
        Some(t) => t.cancelled().await,
        None => std::future::pending().await,
    }
}

// ── Timeout helper ───────────────────────────────────────────────────────────

/// Receive a message from a flume channel with an optional timeout and
/// optional cancellation token.
///
/// Returns `Error::Timeout` if the deadline expires, `Error::Cancelled` if
/// the token fires, or `Error::Transport` if the channel is closed.
pub(crate) async fn recv_with_timeout(
    rx: &flume::Receiver<Result<Message>>,
    timeout: Option<Duration>,
    cancel: Option<&CancellationToken>,
) -> Result<Message> {
    let recv_fut = rx.recv_async();
    tokio::select! {
        biased;
        _ = cancelled_or_pending(cancel) => {
            Err(Error::Cancelled)
        }
        result = async {
            match timeout {
                Some(d) => match tokio::time::timeout(d, recv_fut).await {
                    Ok(Ok(msg)) => msg,
                    Ok(Err(_)) => Err(Error::Transport("message channel closed".into())),
                    Err(_) => Err(Error::Timeout(format!("read timed out after {}s", d.as_secs_f64()))),
                },
                None => match recv_fut.await {
                    Ok(msg) => msg,
                    Err(_) => Err(Error::Transport("message channel closed".into())),
                },
            }
        } => result,
    }
}

// ── Shared turn stream helper ─────────────────────────────────────────────────

/// Read messages from the receiver until a `Result` message or error,
/// then clear the turn flag.
fn read_turn_stream<'a>(
    rx: &'a flume::Receiver<Result<Message>>,
    read_timeout: Option<Duration>,
    turn_flag: Arc<AtomicBool>,
    cancel: Option<CancellationToken>,
) -> impl Stream<Item = Result<Message>> + 'a {
    async_stream::stream! {
        loop {
            match recv_with_timeout(rx, read_timeout, cancel.as_ref()).await {
                Ok(msg) => {
                    let is_result = matches!(&msg, Message::Result(_));
                    yield Ok(msg);
                    if is_result {
                        break;
                    }
                }
                Err(e) => {
                    yield Err(e);
                    break;
                }
            }
        }
        turn_flag.store(false, Ordering::Release);
    }
}

// ── Client ───────────────────────────────────────────────────────────────────

/// A stateful Claude Code client that manages a persistent session.
///
/// # Lifecycle
///
/// 1. Create with [`Client::new(config)`](Client::new) or [`Client::with_transport(config, transport)`](Client::with_transport)
/// 2. Call [`connect()`](Client::connect) to spawn the CLI and read the init message
/// 3. Use [`send()`](Client::send) to send prompts and stream responses
/// 4. Call [`close()`](Client::close) to shut down cleanly
pub struct Client {
    config: ClientConfig,
    transport: Arc<dyn Transport>,
    session_id: Option<String>,
    message_rx: Option<flume::Receiver<Result<Message>>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    turn_active: Arc<AtomicBool>,
    /// Pending outbound control requests awaiting responses.
    pending_control: Arc<DashMap<String, oneshot::Sender<serde_json::Value>>>,
    /// Counter for generating unique request IDs.
    request_counter: Arc<AtomicU64>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("session_id", &self.session_id)
            .field("connected", &self.is_connected())
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Create a new client with the given configuration.
    ///
    /// This does NOT start the CLI — call [`connect()`](Client::connect) next.
    /// Validates the config (e.g., `cwd` existence) and discovers the CLI binary.
    ///
    /// # Errors
    ///
    /// Returns [`Error::CliNotFound`] if the CLI binary cannot be discovered,
    /// or [`Error::Config`] if the configuration is invalid.
    pub fn new(config: ClientConfig) -> Result<Self> {
        config.validate()?;
        let transport = Arc::new(CliTransport::from_config(&config)?);
        Ok(Self {
            config,
            transport,
            session_id: None,
            message_rx: None,
            shutdown_tx: None,
            turn_active: Arc::new(AtomicBool::new(false)),
            pending_control: Arc::new(DashMap::new()),
            request_counter: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Create a client with a custom transport (useful for testing).
    pub fn with_transport(config: ClientConfig, transport: Arc<dyn Transport>) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            transport,
            session_id: None,
            message_rx: None,
            shutdown_tx: None,
            turn_active: Arc::new(AtomicBool::new(false)),
            pending_control: Arc::new(DashMap::new()),
            request_counter: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Connect to the CLI and return the session info from the init message.
    ///
    /// This spawns the CLI process (or connects to the mock transport) and
    /// starts the background reader task. The entire connect sequence
    /// (transport connect + init message read) is subject to `connect_timeout`.
    pub async fn connect(&mut self) -> Result<SessionInfo> {
        let timeout = self.config.connect_timeout;
        let result = match timeout {
            Some(d) => tokio::time::timeout(d, self.connect_inner())
                .await
                .map_err(|_| {
                    Error::Timeout(format!("connect timed out after {}s", d.as_secs_f64()))
                })?,
            None => self.connect_inner().await,
        };
        if result.is_err() {
            // Clean up: stop background task + kill CLI process.
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
            self.message_rx.take();
            let _ = self.transport.close().await;
        }
        result
    }

    async fn connect_inner(&mut self) -> Result<SessionInfo> {
        self.transport.connect().await?;

        // Set up the message routing pipeline.
        let (msg_tx, msg_rx) = flume::bounded(1024);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let transport = Arc::clone(&self.transport);
        let message_callback = self.config.message_callback.clone();
        let pending_control = Arc::clone(&self.pending_control);

        // Move hooks into the background task for hook dispatch.
        let hooks: Vec<crate::hooks::HookMatcher> = std::mem::take(&mut self.config.hooks);
        let default_hook_timeout = self.config.default_hook_timeout;
        let hook_transport = Arc::clone(&self.transport);

        // Capture permission callback for the background task.
        let can_use_tool = self.config.can_use_tool.clone();
        let perm_transport = Arc::clone(&self.transport);

        // Cancellation token for cooperative consumer-side abort.
        let cancel_token = self.config.cancellation_token.clone();

        // Shared session_id for the background task.
        // Updated after the init message is parsed so subsequent hook dispatches
        // carry the real session ID rather than None.
        let shared_session_id: Arc<std::sync::Mutex<Option<String>>> =
            Arc::new(std::sync::Mutex::new(None));
        let hook_session_id = Arc::clone(&shared_session_id);

        // Spawn background reader task.
        tokio::spawn(async move {
            let mut stream = transport.read_messages();
            let mut shutdown = shutdown_rx;

            loop {
                tokio::select! {
                    biased;
                    _ = &mut shutdown => break,
                    _ = cancelled_or_pending(cancel_token.as_ref()) => break,
                    item = stream.next() => {
                        match item {
                            Some(Ok(value)) => {
                                // Route control_response messages to pending senders.
                                if value.get("type").and_then(|v| v.as_str()) == Some("control_response") {
                                    if let Some(req_id) = value.get("request_id").and_then(|v| v.as_str()) {
                                        if let Some((_, tx)) = pending_control.remove(req_id) {
                                            let _ = tx.send(value);
                                        }
                                    }
                                    continue;
                                }

                                // Route hook_request messages to registered hooks.
                                if value.get("type").and_then(|v| v.as_str()) == Some("hook_request") {
                                    if let Ok(req) = serde_json::from_value::<crate::hooks::HookRequest>(value) {
                                        let sid = hook_session_id
                                            .lock()
                                            .expect("session_id lock")
                                            .clone();
                                        let output = crate::hooks::dispatch_hook(
                                            &req,
                                            &hooks,
                                            default_hook_timeout,
                                            sid,
                                        ).await;
                                        let response = crate::hooks::HookResponse::from_output(
                                            req.request_id,
                                            output,
                                        );
                                        if let Ok(json) = serde_json::to_string(&response) {
                                            let _ = hook_transport.write(&json).await;
                                        }
                                    }
                                    continue;
                                }

                                // Route permission_request messages to the can_use_tool callback.
                                if value.get("type").and_then(|v| v.as_str()) == Some("permission_request") {
                                    if let Some(ref callback) = can_use_tool {
                                        if let Ok(req) = serde_json::from_value::<crate::permissions::ControlRequest>(value) {
                                            let crate::permissions::ControlRequestData::PermissionRequest {
                                                ref tool_name,
                                                ref tool_input,
                                                ref tool_use_id,
                                                ref suggestions,
                                            } = req.request;
                                            let sid = hook_session_id
                                                .lock()
                                                .expect("session_id lock")
                                                .clone()
                                                .unwrap_or_default();
                                            let ctx = crate::permissions::PermissionContext {
                                                tool_use_id: tool_use_id.clone(),
                                                session_id: sid,
                                                request_id: req.request_id.clone(),
                                                suggestions: suggestions.clone(),
                                            };
                                            let decision = callback(tool_name, tool_input, ctx).await;
                                            let response = crate::permissions::ControlResponse {
                                                kind: "permission_response".into(),
                                                request_id: req.request_id,
                                                result: crate::permissions::ControlResponseResult::from(decision),
                                            };
                                            if let Ok(json) = serde_json::to_string(&response) {
                                                let _ = perm_transport.write(&json).await;
                                            }
                                        }
                                    } else {
                                        // No can_use_tool callback configured. Send a deny
                                        // response so the CLI doesn't hang waiting forever.
                                        let deny_response = serde_json::json!({
                                            "kind": "permission_response",
                                            "request_id": value.get("request_id")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or(""),
                                            "result": {
                                                "type": "deny",
                                                "message": "no permission callback configured"
                                            }
                                        });
                                        if let Ok(json) = serde_json::to_string(&deny_response) {
                                            let _ = perm_transport.write(&json).await;
                                        }
                                        let _ = msg_tx.send(Err(Error::ControlProtocol(
                                            "received permission_request but no can_use_tool \
                                             callback is configured — set can_use_tool on \
                                             ClientConfig or use a PermissionMode that does not \
                                             require interactive approval"
                                                .into(),
                                        )));
                                    }
                                    continue;
                                }

                                // Parse the JSON value into a Message.
                                let msg: Message = match serde_json::from_value(value) {
                                    Ok(m) => m,
                                    Err(e) => {
                                        let _ = msg_tx.send(Err(Error::Json(e)));
                                        continue;
                                    }
                                };

                                // Apply the message callback.
                                let msg = match apply_callback(msg, message_callback.as_ref()) {
                                    Some(m) => m,
                                    None => continue, // Filtered out
                                };

                                if msg_tx.send(Ok(msg)).is_err() {
                                    break; // Receiver dropped
                                }
                            }
                            Some(Err(e)) => {
                                let _ = msg_tx.send(Err(e));
                            }
                            None => break, // Stream ended
                        }
                    }
                }
            }
        });

        self.message_rx = Some(msg_rx);
        self.shutdown_tx = Some(shutdown_tx);

        // Wait for the system/init message.
        // The overall connect_timeout is enforced by the caller, so we wait
        // indefinitely here (the outer timeout will cancel us if needed).
        let init_msg = self
            .message_rx
            .as_ref()
            .unwrap()
            .recv_async()
            .await
            .map_err(|_| Error::Transport("connection closed before init message".into()))?
            .map_err(|e| Error::Transport(format!("error reading init message: {e}")))?;

        if let Message::System(ref sys) = init_msg {
            let info = SessionInfo::try_from(sys)?;
            self.session_id = Some(info.session_id.clone());
            // Propagate the session ID to the background task so hook
            // dispatches after this point carry the real session ID.
            *shared_session_id.lock().expect("session_id lock") = Some(info.session_id.clone());
            Ok(info)
        } else {
            Err(Error::ControlProtocol(format!(
                "expected system/init as first message, got: {init_msg:?}"
            )))
        }
    }

    /// Send a text prompt and return a stream of response messages.
    ///
    /// The stream yields messages until a `Result` message is received
    /// (which terminates the turn).
    pub fn send(
        &self,
        prompt: impl Into<String>,
    ) -> Result<impl Stream<Item = Result<Message>> + '_> {
        let prompt = prompt.into();
        let rx = self.message_rx.as_ref().ok_or(Error::NotConnected)?;
        let transport = Arc::clone(&self.transport);

        // Guard against concurrent turns.
        if self
            .turn_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Error::ControlProtocol("turn already in progress".into()));
        }
        let turn_flag = Arc::clone(&self.turn_active);
        let read_timeout = self.config.read_timeout;
        let cancel = self.config.cancellation_token.clone();

        Ok(async_stream::stream! {
            // Write the prompt to stdin.
            if let Err(e) = transport.write(&prompt).await {
                turn_flag.store(false, Ordering::Release);
                yield Err(e);
                return;
            }

            let inner = read_turn_stream(rx, read_timeout, turn_flag, cancel);
            tokio::pin!(inner);
            while let Some(item) = inner.next().await {
                yield item;
            }
        })
    }

    /// Send structured content blocks (text + images) and return a stream of
    /// response messages.
    ///
    /// This is the multi-modal equivalent of [`send()`](Client::send). Content
    /// is serialised as a JSON user message and written to the CLI's stdin.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if `content` is empty, or [`Error::NotConnected`]
    /// if the client is not connected.
    pub fn send_content(
        &self,
        content: Vec<UserContent>,
    ) -> Result<impl Stream<Item = Result<Message>> + '_> {
        if content.is_empty() {
            return Err(Error::Config("content must not be empty".into()));
        }

        let rx = self.message_rx.as_ref().ok_or(Error::NotConnected)?;
        let transport = Arc::clone(&self.transport);

        // Guard against concurrent turns.
        if self
            .turn_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Error::ControlProtocol("turn already in progress".into()));
        }
        let turn_flag = Arc::clone(&self.turn_active);
        let read_timeout = self.config.read_timeout;
        let cancel = self.config.cancellation_token.clone();

        Ok(async_stream::stream! {
            // Serialize content blocks as a JSON user message.
            let user_message = serde_json::json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": content
                }
            });
            let json = match serde_json::to_string(&user_message) {
                Ok(j) => j,
                Err(e) => {
                    turn_flag.store(false, Ordering::Release);
                    yield Err(Error::Json(e));
                    return;
                }
            };

            if let Err(e) = transport.write(&json).await {
                turn_flag.store(false, Ordering::Release);
                yield Err(e);
                return;
            }

            let inner = read_turn_stream(rx, read_timeout, turn_flag, cancel);
            tokio::pin!(inner);
            while let Some(item) = inner.next().await {
                yield item;
            }
        })
    }

    /// Return a stream of all incoming messages (without sending a prompt).
    ///
    /// Useful for consuming messages from a resumed session.
    pub fn receive_messages(&self) -> Result<impl Stream<Item = Result<Message>> + '_> {
        let rx = self.message_rx.as_ref().ok_or(Error::NotConnected)?;
        let read_timeout = self.config.read_timeout;
        let cancel = self.config.cancellation_token.clone();

        Ok(async_stream::stream! {
            loop {
                match recv_with_timeout(rx, read_timeout, cancel.as_ref()).await {
                    Ok(msg) => yield Ok(msg),
                    Err(e) if matches!(e, Error::Transport(_)) => break, // Channel closed
                    Err(e) => {
                        yield Err(e);
                        break;
                    }
                }
            }
        })
    }

    /// Send an interrupt signal to the CLI (SIGINT).
    pub async fn interrupt(&self) -> Result<()> {
        self.transport.interrupt().await
    }

    /// Respond to a permission request from the CLI.
    ///
    /// When the CLI asks for permission to use a tool, this method sends
    /// the decision back via the control protocol.
    pub async fn respond_to_permission(
        &self,
        request_id: &str,
        decision: crate::permissions::PermissionDecision,
    ) -> Result<()> {
        use crate::permissions::{ControlResponse, ControlResponseResult};

        let response = ControlResponse {
            kind: "permission_response".into(),
            request_id: request_id.to_string(),
            result: ControlResponseResult::from(decision),
        };
        let json = serde_json::to_string(&response).map_err(Error::Json)?;
        self.transport.write(&json).await
    }

    // ── Dynamic control ─────────────────────────────────────────────────

    /// Send a control request to the CLI and wait for the response.
    ///
    /// This is the low-level mechanism for dynamic mid-session control.
    /// The request is wrapped in a `{"type": "control_request", ...}` envelope
    /// and written to stdin. The background reader routes the matching
    /// `control_response` back via a `oneshot` channel.
    async fn send_control_request(&self, request: serde_json::Value) -> Result<serde_json::Value> {
        let counter = self.request_counter.fetch_add(1, Ordering::Relaxed);
        let request_id = format!("sdk_req_{counter}");

        let (tx, rx) = oneshot::channel();
        self.pending_control.insert(request_id.clone(), tx);

        let envelope = serde_json::json!({
            "type": "control_request",
            "request_id": request_id,
            "request": request
        });
        let json = serde_json::to_string(&envelope).map_err(Error::Json)?;
        self.transport.write(&json).await?;

        let timeout = self.config.control_request_timeout;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => {
                self.pending_control.remove(&request_id);
                Err(Error::ControlProtocol(
                    "control response channel closed".into(),
                ))
            }
            Err(_) => {
                self.pending_control.remove(&request_id);
                Err(Error::Timeout(format!(
                    "control request timed out after {}s",
                    timeout.as_secs_f64()
                )))
            }
        }
    }

    /// Dynamically change the model used for subsequent turns.
    ///
    /// Pass `None` to revert to the session's default model.
    ///
    /// # Errors
    ///
    /// Returns an error if the CLI rejects the model change or the control
    /// protocol fails.
    pub async fn set_model(&self, model: Option<&str>) -> Result<()> {
        self.send_control_request(serde_json::json!({
            "subtype": "set_model",
            "model": model
        }))
        .await?;
        Ok(())
    }

    /// Dynamically change the permission mode for the current session.
    ///
    /// # Errors
    ///
    /// Returns an error if the CLI rejects the mode change or the control
    /// protocol fails.
    pub async fn set_permission_mode(&self, mode: PermissionMode) -> Result<()> {
        self.send_control_request(serde_json::json!({
            "subtype": "set_permission_mode",
            "mode": mode.as_cli_flag()
        }))
        .await?;
        Ok(())
    }

    /// Write raw data to the transport's stdin.
    ///
    /// This is a low-level method used by free functions like
    /// [`query_stream_with_content`](crate::query_stream_with_content).
    pub(crate) async fn transport_write(&self, data: &str) -> Result<()> {
        self.transport.write(data).await
    }

    /// Take ownership of the message receiver (for use in `query_stream`).
    ///
    /// After calling this, `receive_messages()` and `send()` will return
    /// `NotConnected`.
    pub(crate) fn take_message_rx(&mut self) -> Option<flume::Receiver<Result<Message>>> {
        self.message_rx.take()
    }

    /// Returns the configured read timeout.
    #[must_use]
    pub fn read_timeout(&self) -> Option<Duration> {
        self.config.read_timeout
    }

    /// Returns the session ID if connected.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Returns `true` if the client is connected.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.transport.is_ready()
    }

    /// Close the client and shut down the CLI process.
    ///
    /// Returns the CLI process exit code if available. After calling this,
    /// the `Drop` warning will not fire.
    pub async fn close(&mut self) -> Result<Option<i32>> {
        // Signal the background task to stop.
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        // Drop the message receiver so the Drop impl knows we cleaned up.
        self.message_rx.take();
        self.transport.close().await
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if self.shutdown_tx.is_some() || self.message_rx.is_some() {
            tracing::warn!(
                "claude_cli_sdk::Client dropped without calling close(). \
                 Resources may not be cleaned up properly."
            );
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClientConfig;

    #[cfg(feature = "testing")]
    use crate::testing::{ScenarioBuilder, assistant_text};

    fn test_config() -> ClientConfig {
        ClientConfig::builder().prompt("test").build()
    }

    #[cfg(feature = "testing")]
    #[tokio::test]
    async fn client_connect_and_receive_init() {
        let transport = ScenarioBuilder::new("test-session")
            .exchange(vec![assistant_text("Hello!")])
            .build();
        let transport = Arc::new(transport);

        let mut client = Client::with_transport(test_config(), transport).unwrap();
        let info = client.connect().await.unwrap();

        assert_eq!(info.session_id, "test-session");
        assert!(client.is_connected());
        assert_eq!(client.session_id(), Some("test-session"));
    }

    #[cfg(feature = "testing")]
    #[tokio::test]
    async fn client_send_yields_messages() {
        let transport = ScenarioBuilder::new("s1")
            .exchange(vec![assistant_text("response")])
            .build();
        let transport = Arc::new(transport);

        let mut client = Client::with_transport(test_config(), transport).unwrap();
        client.connect().await.unwrap();

        let stream = client.send("hello").unwrap();
        tokio::pin!(stream);

        let mut messages = Vec::new();
        while let Some(msg) = stream.next().await {
            messages.push(msg.unwrap());
        }

        // Should get assistant + result
        assert_eq!(messages.len(), 2);
        assert!(matches!(&messages[0], Message::Assistant(_)));
        assert!(matches!(&messages[1], Message::Result(_)));
    }

    #[cfg(feature = "testing")]
    #[tokio::test]
    async fn client_close_succeeds() {
        let transport = ScenarioBuilder::new("s1").build();
        let transport = Arc::new(transport);

        let mut client = Client::with_transport(test_config(), transport).unwrap();
        client.connect().await.unwrap();
        assert!(client.close().await.is_ok());
    }

    #[cfg(feature = "testing")]
    #[tokio::test]
    async fn client_message_callback_filters() {
        use crate::callback::MessageCallback;

        // Filter out assistant messages.
        let callback: MessageCallback = Arc::new(|msg| match &msg {
            Message::Assistant(_) => None,
            _ => Some(msg),
        });

        let config = ClientConfig::builder()
            .prompt("test")
            .message_callback(callback)
            .build();

        let transport = ScenarioBuilder::new("s1")
            .exchange(vec![assistant_text("filtered")])
            .build();
        let transport = Arc::new(transport);

        let mut client = Client::with_transport(config, transport).unwrap();
        client.connect().await.unwrap();

        let stream = client.send("hello").unwrap();
        tokio::pin!(stream);

        let mut messages = Vec::new();
        while let Some(msg) = stream.next().await {
            messages.push(msg.unwrap());
        }

        // Only result (assistant was filtered).
        assert_eq!(messages.len(), 1);
        assert!(matches!(&messages[0], Message::Result(_)));
    }

    #[cfg(feature = "testing")]
    #[test]
    fn client_debug_before_connect() {
        let transport = Arc::new(crate::testing::MockTransport::new());
        let client = Client::with_transport(test_config(), transport).unwrap();
        let debug = format!("{client:?}");
        assert!(debug.contains("Client"));
    }

    // ── Timeout tests ────────────────────────────────────────────────────

    #[cfg(feature = "testing")]
    #[tokio::test]
    async fn client_connect_timeout_fires() {
        use crate::testing::MockTransport;

        let transport = MockTransport::new();
        // Set connect delay longer than timeout.
        transport.set_connect_delay(Duration::from_secs(5));
        let transport = Arc::new(transport);

        let config = ClientConfig::builder()
            .prompt("test")
            .connect_timeout(Some(Duration::from_millis(50)))
            .build();

        let mut client = Client::with_transport(config, transport).unwrap();
        let result = client.connect().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Timeout(_)));
    }

    #[cfg(feature = "testing")]
    #[tokio::test]
    async fn client_read_timeout_fires() {
        // Build a scenario with init + assistant, but add a large recv_delay
        // so the assistant message arrives after the read timeout.
        let transport = ScenarioBuilder::new("s1")
            .exchange(vec![assistant_text("delayed")])
            .build();
        // Delay each message by 5 seconds — way longer than our 50ms timeout.
        // The init message also gets this delay, but the connect has no timeout
        // wrapping for the recv path since connect_timeout is set to None here
        // and connect_inner waits indefinitely for init.
        // Actually, we need init to arrive fast but subsequent messages slow.
        // The MockTransport applies recv_delay to ALL messages including init.
        // So set connect_timeout high enough and read_timeout low.
        transport.set_recv_delay(Duration::from_millis(200));
        let transport = Arc::new(transport);

        let config = ClientConfig::builder()
            .prompt("test")
            .connect_timeout(Some(Duration::from_secs(5)))
            .read_timeout(Some(Duration::from_millis(50)))
            .build();

        let mut client = Client::with_transport(config, transport).unwrap();
        client.connect().await.unwrap();

        let stream = client.send("hello").unwrap();
        tokio::pin!(stream);

        let mut got_timeout = false;
        while let Some(msg) = stream.next().await {
            if let Err(Error::Timeout(_)) = msg {
                got_timeout = true;
                break;
            }
        }
        assert!(got_timeout, "expected a timeout error");
    }

    #[cfg(feature = "testing")]
    #[tokio::test]
    async fn client_permission_callback_invoked_and_responds() {
        use crate::permissions::{CanUseToolCallback, PermissionDecision};
        use crate::testing::MockTransport;
        use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

        let invoked = Arc::new(AtomicBool::new(false));
        let invoked_clone = Arc::clone(&invoked);

        let callback: CanUseToolCallback = Arc::new(move |tool_name: &str, _input, _ctx| {
            let invoked = Arc::clone(&invoked_clone);
            let tool = tool_name.to_owned();
            Box::pin(async move {
                invoked.store(true, AtomicOrdering::Release);
                assert_eq!(tool, "Bash");
                PermissionDecision::allow()
            })
        });

        let config = ClientConfig::builder()
            .prompt("test")
            .can_use_tool(callback)
            .build();

        let transport = MockTransport::new();
        // Enqueue: init, permission_request, assistant, result
        transport.enqueue(r#"{"type":"system","subtype":"init","session_id":"s1","cwd":"/","tools":[],"mcp_servers":[],"model":"m"}"#);
        transport.enqueue(r#"{"type":"permission_request","request_id":"perm-1","request":{"type":"permission_request","tool_name":"Bash","tool_input":{"command":"ls"},"tool_use_id":"tu-1","suggestions":["allow_once"]}}"#);
        transport.enqueue(&serde_json::to_string(&crate::testing::assistant_text("done")).unwrap());
        transport.enqueue(r#"{"type":"result","subtype":"success","session_id":"s1","is_error":false,"num_turns":1,"usage":{}}"#);
        let transport = Arc::new(transport);

        let mut client = Client::with_transport(config, transport.clone()).unwrap();
        client.connect().await.unwrap();

        let stream = client.send("hello").unwrap();
        tokio::pin!(stream);
        let mut messages = Vec::new();
        while let Some(msg) = stream.next().await {
            messages.push(msg.unwrap());
        }

        // Callback should have been invoked.
        assert!(
            invoked.load(AtomicOrdering::Acquire),
            "permission callback was not invoked"
        );

        // Permission request should NOT leak as a Message — only assistant + result.
        assert_eq!(messages.len(), 2);
        assert!(matches!(&messages[0], Message::Assistant(_)));
        assert!(matches!(&messages[1], Message::Result(_)));

        // Verify a permission_response was written back.
        let written = transport.written_lines();
        let perm_responses: Vec<_> = written
            .iter()
            .filter(|line| line.contains("permission_response"))
            .collect();
        assert_eq!(
            perm_responses.len(),
            1,
            "expected exactly one permission_response written"
        );
        let resp: serde_json::Value = serde_json::from_str(perm_responses[0]).unwrap();
        assert_eq!(resp["kind"], "permission_response");
        assert_eq!(resp["request_id"], "perm-1");
        assert_eq!(resp["result"]["type"], "allow");
    }

    #[cfg(feature = "testing")]
    #[tokio::test]
    async fn client_permission_callback_deny_writes_deny_response() {
        use crate::permissions::{CanUseToolCallback, PermissionDecision};
        use crate::testing::MockTransport;

        let callback: CanUseToolCallback = Arc::new(|_tool_name, _input, _ctx| {
            Box::pin(async { PermissionDecision::deny("not allowed") })
        });

        let config = ClientConfig::builder()
            .prompt("test")
            .can_use_tool(callback)
            .build();

        let transport = MockTransport::new();
        transport.enqueue(r#"{"type":"system","subtype":"init","session_id":"s1","cwd":"/","tools":[],"mcp_servers":[],"model":"m"}"#);
        transport.enqueue(r#"{"type":"permission_request","request_id":"perm-2","request":{"type":"permission_request","tool_name":"Write","tool_input":{"path":"/etc/shadow"},"tool_use_id":"tu-2","suggestions":[]}}"#);
        transport
            .enqueue(&serde_json::to_string(&crate::testing::assistant_text("denied")).unwrap());
        transport.enqueue(r#"{"type":"result","subtype":"success","session_id":"s1","is_error":false,"num_turns":1,"usage":{}}"#);
        let transport = Arc::new(transport);

        let mut client = Client::with_transport(config, transport.clone()).unwrap();
        client.connect().await.unwrap();

        let stream = client.send("hello").unwrap();
        tokio::pin!(stream);
        let mut messages = Vec::new();
        while let Some(msg) = stream.next().await {
            messages.push(msg.unwrap());
        }

        // Verify deny response was written.
        let written = transport.written_lines();
        let perm_responses: Vec<_> = written
            .iter()
            .filter(|line| line.contains("permission_response"))
            .collect();
        assert_eq!(perm_responses.len(), 1);
        let resp: serde_json::Value = serde_json::from_str(perm_responses[0]).unwrap();
        assert_eq!(resp["kind"], "permission_response");
        assert_eq!(resp["request_id"], "perm-2");
        assert_eq!(resp["result"]["type"], "deny");
        assert_eq!(resp["result"]["message"], "not allowed");
    }

    #[cfg(feature = "testing")]
    #[tokio::test]
    async fn client_permission_request_without_callback_yields_error() {
        use crate::testing::MockTransport;

        // No can_use_tool callback configured.
        let config = ClientConfig::builder().prompt("test").build();

        let transport = MockTransport::new();
        transport.enqueue(r#"{"type":"system","subtype":"init","session_id":"s1","cwd":"/","tools":[],"mcp_servers":[],"model":"m"}"#);
        transport.enqueue(r#"{"type":"permission_request","request_id":"perm-3","request":{"type":"permission_request","tool_name":"Bash","tool_input":{"command":"ls"},"tool_use_id":"tu-3","suggestions":[]}}"#);
        transport
            .enqueue(&serde_json::to_string(&crate::testing::assistant_text("after")).unwrap());
        transport.enqueue(r#"{"type":"result","subtype":"success","session_id":"s1","is_error":false,"num_turns":1,"usage":{}}"#);
        let transport = Arc::new(transport);

        let mut client = Client::with_transport(config, transport).unwrap();
        client.connect().await.unwrap();

        let stream = client.send("hello").unwrap();
        tokio::pin!(stream);

        let mut got_error = false;
        let mut messages = Vec::new();
        while let Some(result) = stream.next().await {
            match result {
                Ok(msg) => messages.push(msg),
                Err(Error::ControlProtocol(ref msg)) if msg.contains("can_use_tool") => {
                    got_error = true;
                }
                Err(e) => panic!("unexpected error: {e}"),
            }
        }

        assert!(
            got_error,
            "should have received a ControlProtocol error for missing callback"
        );
    }

    #[tokio::test]
    async fn recv_with_timeout_respects_cancellation_token() {
        let (tx, rx) = flume::unbounded::<Result<Message>>();
        let token = CancellationToken::new();

        // Cancel immediately.
        token.cancel();

        let result = recv_with_timeout(&rx, None, Some(&token)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_cancelled());

        // Sender is still alive — we didn't get a transport error.
        drop(tx);
    }

    #[tokio::test]
    async fn recv_with_timeout_none_cancel_still_works() {
        let (_tx, rx) = flume::unbounded::<Result<Message>>();

        // With no cancel token and a short timeout, we should get a timeout error.
        let result = recv_with_timeout(&rx, Some(Duration::from_millis(10)), None).await;
        assert!(matches!(result, Err(Error::Timeout(_))));
    }

    #[cfg(feature = "testing")]
    #[tokio::test]
    async fn client_read_timeout_none_waits() {
        // MockTransport with recv_delay < reasonable wait, read_timeout None.
        let transport = ScenarioBuilder::new("s1")
            .exchange(vec![assistant_text("delayed")])
            .build();
        transport.set_recv_delay(Duration::from_millis(50));
        let transport = Arc::new(transport);

        let config = ClientConfig::builder()
            .prompt("test")
            .read_timeout(None)
            .build();

        let mut client = Client::with_transport(config, transport).unwrap();
        client.connect().await.unwrap();

        let stream = client.send("hello").unwrap();
        tokio::pin!(stream);

        let mut messages = Vec::new();
        while let Some(msg) = stream.next().await {
            messages.push(msg.unwrap());
        }

        // Should get assistant + result even with delay since no timeout.
        assert_eq!(messages.len(), 2);
    }
}
