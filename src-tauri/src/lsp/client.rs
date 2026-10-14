//! Minimal JSON-RPC 2.0 client for one language server child, framed the way LSP specifies.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex as StdMutex;
use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, oneshot, Mutex};

use crate::lsp::{Error, Result};

pub(crate) const NOTIFICATION_CAPACITY: usize = 256;
pub(crate) const MAX_FRAME: usize = 64 * 1024 * 1024;
pub(crate) const METHOD_NOT_FOUND: i64 = -32601;

/// One language server process, with request/response correlation and notification fan-out.
pub struct Client {
    child: Mutex<Option<Child>>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Result<Value>>>>>,
    notifications: broadcast::Sender<Value>,
    alive: Arc<AtomicBool>,
    next_id: AtomicI64,
}

impl Client {
    pub async fn spawn(bin: &Path, root: &Path) -> Result<Self> {
        let mut child = Command::new(bin)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::ServerMissing {
                searched: format!("{} ({e})", bin.display()),
            })?;

        let stdin = child.stdin.take().ok_or_else(|| Error::Protocol("language server has no stdin".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| Error::Protocol("language server has no stdout".into()))?;

        let stdin = Arc::new(Mutex::new(stdin));
        let pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Result<Value>>>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let (notifications, _) = broadcast::channel(NOTIFICATION_CAPACITY);
        let alive = Arc::new(AtomicBool::new(true));
        let alive_for_reader = alive.clone();
        let stdin_for_reader = stdin.clone();
        let pending_for_reader = pending.clone();
        let notifications_for_reader = notifications.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let pending = pending_for_reader;
            let notifications = notifications_for_reader;
            let alive = alive_for_reader;
            let stdin = stdin_for_reader;

            loop {
                match read_frame(&mut reader).await {
                    Ok(Some(message)) => {
                        let id_present = message.get("id").map(|v| !v.is_null()).unwrap_or(false);
                        if id_present {
                            let id = message.get("id").and_then(Value::as_i64).unwrap_or(0);
                            if let Some(sender) = pending.lock().remove(&id) {
                                let _ = sender.send(Ok(message));
                            }
                        } else {
                            if message.get("method").is_some() && message.get("id").is_some() {
                                let reply = json!({
                                    "jsonrpc": "2.0",
                                    "id": message.get("id").cloned().unwrap_or(json!(null)),
                                    "error": { "code": METHOD_NOT_FOUND, "message": "method not supported" }
                                });
                                if let Err(e) = write_frame(&mut *stdin.lock().await, &reply).await {
                                    eprintln!("lsp: reply failed: {e}");
                                }
                            }
                            let _ = notifications.send(message);
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }

            alive.store(false, Ordering::SeqCst);
            pending.lock().clear();
        });

        Ok(Self {
            child: Mutex::new(Some(child)),
            stdin,
            pending,
            notifications,
            alive,
            next_id: AtomicI64::new(1),
        })
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    pub async fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        if !self.is_alive() {
            return Err(Error::Protocol(format!("language server died during {method}")));
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        {
            let mut map = self.pending.lock();
            map.insert(id, tx);
        }

        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let mut stdin = self.stdin.lock().await;

        if let Err(e) = write_frame(&mut *stdin, &message).await {
            self.pending.lock().remove(&id);
            return Err(e);
        }
        drop(stdin);

        let response = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(value))) => value,
            Ok(Ok(Err(e))) => return Err(e),
            Ok(Err(_)) => return Err(Error::Protocol("response channel closed unexpectedly".into())),
            Err(_) => {
                self.pending.lock().remove(&id);
                return Err(Error::Timeout { method: method.to_string() });
            }
        };

        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(Error::Protocol(format!("{method}: {message}")));
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let message = json!({"jsonrpc": "2.0", "method": method, "params": params});
        write_frame(&mut *self.stdin.lock().await, &message).await
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.notifications.subscribe()
    }

    pub async fn shutdown(&self) {
        let mut guard = self.child.lock().await;
        if let Some(mut child) = guard.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), child.kill()).await;
        }
    }
}

pub(crate) async fn read_frame<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line).await?;
        if read == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().ok();
            }
        }
    }

    let length = content_length.ok_or_else(|| Error::Protocol("missing Content-Length".into()))?;
    if length > MAX_FRAME {
        return Err(Error::Protocol(format!("frame of {length} bytes is too large")));
    }

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;
    Ok(Some(serde_json::from_slice(body.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&body))?))
}

pub(crate) async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, message: &Value) -> Result<()> {
    let body = serde_json::to_vec(message)?;
    writer.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}
