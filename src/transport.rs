//! Transport layer — spawning and communicating with the Claude CLI.
//!
//! The [`Transport`] trait abstracts the communication channel between the SDK
//! and the Claude Code process.  [`CliTransport`] is the production
//! implementation that spawns the CLI as a subprocess and uses NDJSON over
//! stdin/stdout.
//!
//! # Architecture
//!
//! `CliTransport::connect()` spawns the CLI, then starts a background reader
//! task that reads NDJSON lines from stdout and forwards parsed JSON values
//! through a `tokio::sync::mpsc` channel.  `read_messages()` returns a
//! `ReceiverStream` — no mutex is held on the hot path.

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures_core::Stream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;

use crate::errors::{Error, Result};

// ── Transport Trait ──────────────────────────────────────────────────────────

/// Abstraction over the communication channel to the Claude CLI.
///
/// Implementations must be `Send + Sync` for use across async tasks.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Establish the connection (e.g., spawn the CLI process).
    async fn connect(&self) -> Result<()>;

    /// Write a line to the CLI's stdin.
    async fn write(&self, data: &str) -> Result<()>;

    /// Return a stream of parsed JSON values from the CLI's stdout.
    fn read_messages(&self) -> Pin<Box<dyn Stream<Item = Result<serde_json::Value>> + Send>>;

    /// Signal end-of-input to the CLI (close stdin).
    async fn end_input(&self) -> Result<()>;

    /// Send an interrupt signal (SIGINT on Unix).
    async fn interrupt(&self) -> Result<()>;

    /// Returns `true` if the transport is connected and ready.
    fn is_ready(&self) -> bool;

    /// Close the transport and wait for the process to exit.
    ///
    /// Returns the exit code if available.
    async fn close(&self) -> Result<Option<i32>>;
}

// ── CliTransport ─────────────────────────────────────────────────────────────

/// Production transport that spawns the Claude CLI as a subprocess.
///
/// Uses NDJSON over stdin/stdout. A background reader task forwards parsed
/// JSON values through a bounded channel.
pub struct CliTransport {
    cli_path: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    env: HashMap<String, String>,
    close_timeout: Option<Duration>,
    end_input_on_connect: bool,
    // `process` uses `std::sync::Mutex` because it must be accessed in the
    // synchronous `Drop` impl. The remaining fields use `tokio::sync::Mutex`
    // because they are only accessed in async contexts.
    process: std::sync::Mutex<Option<Child>>,
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    message_rx: std::sync::Mutex<Option<mpsc::Receiver<Result<serde_json::Value>>>>,
    stderr_callback: Option<Arc<dyn Fn(String) + Send + Sync>>,
    reader_handle: tokio::sync::Mutex<Option<JoinHandle<()>>>,
    ready: AtomicBool,
}

impl std::fmt::Debug for CliTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CliTransport")
            .field("cli_path", &self.cli_path)
            .field("cwd", &self.cwd)
            .field("ready", &self.ready.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl CliTransport {
    /// Create a new `CliTransport` with the given configuration.
    ///
    /// This does NOT spawn the process — call [`connect()`](Transport::connect) first.
    #[must_use]
    pub fn new(
        cli_path: PathBuf,
        args: Vec<String>,
        cwd: PathBuf,
        env: HashMap<String, String>,
        stderr_callback: Option<Arc<dyn Fn(String) + Send + Sync>>,
    ) -> Self {
        Self {
            cli_path,
            args,
            cwd,
            env,
            close_timeout: None,
            end_input_on_connect: false,
            process: std::sync::Mutex::new(None),
            stdin: tokio::sync::Mutex::new(None),
            message_rx: std::sync::Mutex::new(None),
            stderr_callback,
            reader_handle: tokio::sync::Mutex::new(None),
            ready: AtomicBool::new(false),
        }
    }

    /// Create from a [`ClientConfig`](crate::config::ClientConfig).
    pub fn from_config(config: &crate::config::ClientConfig) -> Result<Self> {
        let cli_path = match &config.cli_path {
            Some(p) => p.clone(),
            None => crate::discovery::find_cli()?,
        };

        let cwd = config
            .cwd
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let args = config.to_cli_args();
        let env = config.to_env();

        let mut transport = Self::new(cli_path, args, cwd, env, config.stderr_callback.clone());
        transport.close_timeout = config.close_timeout;
        transport.end_input_on_connect = config.end_input_on_connect;
        Ok(transport)
    }

    /// Internal connect logic without timeout wrapping.
    async fn connect_inner(&self) -> Result<()> {
        use std::process::Stdio;

        let mut cmd = tokio::process::Command::new(&self.cli_path);
        cmd.args(&self.args)
            .current_dir(&self.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (k, v) in &self.env {
            cmd.env(k, v);
        }

        // Strip Claude Code env vars so the child CLI process doesn't think
        // it's nested inside another Claude Code session. This happens when
        // the SDK is used from a daemon that was itself started from Claude Code.
        // - CLAUDECODE: Presence triggers "cannot launch inside another session" error
        // - CLAUDE_CODE_SSE_PORT: Causes child to connect to parent's SSE port
        // - CLAUDE_CODE_ENTRYPOINT: Inherited entrypoint context from parent
        // NOTE: These removes run AFTER the user env loop so they can never be
        // re-added by a caller passing them through ClientConfig::env.
        cmd.env_remove("CLAUDECODE");
        cmd.env_remove("CLAUDE_CODE_SSE_PORT");
        cmd.env_remove("CLAUDE_CODE_ENTRYPOINT");

        // On Windows, create the child in its own process group so that
        // interrupt signals can target it without affecting the host process.
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;
            cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }

        let mut child = cmd.spawn().map_err(Error::SpawnFailed)?;

        let child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Transport("failed to capture child stdin".into()))?;

        let child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Transport("failed to capture child stdout".into()))?;

        let child_stderr = child.stderr.take();

        // Channel for NDJSON messages from the reader task.
        let (tx, rx) = mpsc::channel(256);

        // Background reader task: read NDJSON lines from stdout.
        let reader_handle = tokio::spawn(async move {
            let reader = BufReader::new(child_stdout);
            let mut lines = reader.lines();

            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let line = line.trim().to_string();
                        if line.is_empty() {
                            continue;
                        }

                        let result =
                            serde_json::from_str::<serde_json::Value>(&line).map_err(|e| {
                                Error::ParseError {
                                    message: e.to_string(),
                                    line: if line.len() > 200 {
                                        format!("{}...", &line[..200])
                                    } else {
                                        line.clone()
                                    },
                                }
                            });

                        if tx.send(result).await.is_err() {
                            break; // Receiver dropped
                        }
                    }
                    Ok(None) => break, // Stream ended
                    Err(e) => {
                        let _ = tx.send(Err(Error::Io(e))).await;
                        break;
                    }
                }
            }
        });

        // Optional stderr reader task.
        if let Some(stderr) = child_stderr {
            let stderr_cb = self.stderr_callback.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(cb) = &stderr_cb {
                        cb(line);
                    }
                }
            });
        }

        // Store state.
        *self.process.lock().expect("process lock") = Some(child);
        *self.stdin.lock().await = Some(child_stdin);
        *self.message_rx.lock().expect("message_rx lock") = Some(rx);
        *self.reader_handle.lock().await = Some(reader_handle);
        self.ready.store(true, Ordering::Release);

        // Close stdin if configured — required for --print mode with stream-json
        // where the CLI waits for stdin EOF before processing.
        if self.end_input_on_connect {
            self.end_input().await?;
        }

        Ok(())
    }
}

#[async_trait]
impl Transport for CliTransport {
    async fn connect(&self) -> Result<()> {
        self.connect_inner().await
    }

    async fn write(&self, data: &str) -> Result<()> {
        let mut guard = self.stdin.lock().await;
        let stdin = guard.as_mut().ok_or(Error::NotConnected)?;
        stdin
            .write_all(data.as_bytes())
            .await
            .map_err(|e| Error::Transport(format!("write failed: {e}")))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| Error::Transport(format!("write newline failed: {e}")))?;
        stdin
            .flush()
            .await
            .map_err(|e| Error::Transport(format!("flush failed: {e}")))?;
        Ok(())
    }

    fn read_messages(&self) -> Pin<Box<dyn Stream<Item = Result<serde_json::Value>> + Send>> {
        // Take the receiver out of the mutex. This can only be called once
        // (subsequent calls return an empty stream).
        let rx = self.message_rx.lock().expect("message_rx lock").take();

        match rx {
            Some(rx) => Box::pin(ReceiverStream::new(rx)),
            None => {
                tracing::debug!("read_messages() called after receiver was already taken");
                Box::pin(tokio_stream::empty())
            }
        }
    }

    async fn end_input(&self) -> Result<()> {
        let mut guard = self.stdin.lock().await;
        // Drop the stdin handle to signal EOF.
        *guard = None;
        Ok(())
    }

    async fn interrupt(&self) -> Result<()> {
        let guard = self.process.lock().expect("process lock");
        if let Some(child) = guard.as_ref() {
            if let Some(pid) = child.id() {
                #[cfg(unix)]
                {
                    use nix::sys::signal::{Signal, kill};
                    use nix::unistd::Pid;
                    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGINT);
                }
                #[cfg(windows)]
                {
                    use windows_sys::Win32::System::Console::{
                        CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent,
                    };
                    // CTRL_BREAK_EVENT is used because CTRL_C_EVENT is ignored by
                    // processes created with CREATE_NEW_PROCESS_GROUP. CTRL_BREAK
                    // is delivered to the specific process group (the child's PID
                    // serves as the group ID).
                    let _ = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) };
                }
            }
        }
        Ok(())
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    async fn close(&self) -> Result<Option<i32>> {
        self.ready.store(false, Ordering::Release);

        // Close stdin to signal the CLI to exit.
        self.end_input().await?;

        // Wait for the reader task to finish.
        if let Some(handle) = self.reader_handle.lock().await.take() {
            let _ = handle.await;
        }

        // Take the child out of the mutex before awaiting.
        let maybe_child = {
            let mut guard = self.process.lock().expect("process lock");
            guard.take()
        };

        let exit_code = if let Some(mut child) = maybe_child {
            match self.close_timeout {
                Some(d) => {
                    match tokio::time::timeout(d, child.wait()).await {
                        Ok(Ok(status)) => status.code(),
                        Ok(Err(e)) => {
                            return Err(Error::Transport(format!("wait failed: {e}")));
                        }
                        Err(_) => {
                            // Timed out waiting — kill the process.
                            let _ = child.kill().await;
                            return Err(Error::Timeout(format!(
                                "close timed out after {}s, process killed",
                                d.as_secs_f64()
                            )));
                        }
                    }
                }
                None => {
                    let status = child
                        .wait()
                        .await
                        .map_err(|e| Error::Transport(format!("wait failed: {e}")))?;
                    status.code()
                }
            }
        } else {
            None
        };

        Ok(exit_code)
    }
}

impl Drop for CliTransport {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.process.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.start_kill();
            }
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_transport_debug() {
        let t = CliTransport::new(
            PathBuf::from("/usr/bin/claude"),
            vec!["--print".into(), "hi".into()],
            PathBuf::from("/tmp"),
            HashMap::new(),
            None,
        );
        let debug = format!("{t:?}");
        assert!(debug.contains("claude"));
        assert!(debug.contains("/tmp"));
    }

    #[test]
    fn cli_transport_not_ready_before_connect() {
        let t = CliTransport::new(
            PathBuf::from("/usr/bin/claude"),
            vec![],
            PathBuf::from("/tmp"),
            HashMap::new(),
            None,
        );
        assert!(!t.is_ready());
    }

    #[test]
    fn cli_transport_read_messages_returns_empty_without_connect() {
        let t = CliTransport::new(
            PathBuf::from("/usr/bin/claude"),
            vec![],
            PathBuf::from("/tmp"),
            HashMap::new(),
            None,
        );
        // Should not panic — returns empty stream.
        let _stream = t.read_messages();
    }

    #[tokio::test]
    async fn cli_transport_write_fails_when_not_connected() {
        let t = CliTransport::new(
            PathBuf::from("/usr/bin/claude"),
            vec![],
            PathBuf::from("/tmp"),
            HashMap::new(),
            None,
        );
        let err = t.write("test").await.unwrap_err();
        assert!(matches!(err, Error::NotConnected));
    }

    #[tokio::test]
    async fn cli_transport_end_input_ok_when_not_connected() {
        let t = CliTransport::new(
            PathBuf::from("/usr/bin/claude"),
            vec![],
            PathBuf::from("/tmp"),
            HashMap::new(),
            None,
        );
        // Should not error — just no-op.
        assert!(t.end_input().await.is_ok());
    }

    #[tokio::test]
    async fn cli_transport_interrupt_ok_when_not_connected() {
        let t = CliTransport::new(
            PathBuf::from("/usr/bin/claude"),
            vec![],
            PathBuf::from("/tmp"),
            HashMap::new(),
            None,
        );
        assert!(t.interrupt().await.is_ok());
    }

    #[tokio::test]
    async fn cli_transport_close_ok_when_not_connected() {
        let t = CliTransport::new(
            PathBuf::from("/usr/bin/claude"),
            vec![],
            PathBuf::from("/tmp"),
            HashMap::new(),
            None,
        );
        let result = t.close().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
