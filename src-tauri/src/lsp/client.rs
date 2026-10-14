//! Minimal JSON-RPC 2.0 client for one language server child, framed the way LSP specifies.
//!
//! Deliberately no `lsp-types`/`tower-lsp`: v0 asks two questions (definition, didOpen), so the wire format
//! is ~100 lines and the types are the ones we already need.

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

/// Notifications kept for whoever is listening (`$/progress`, `window/logMessage`). Bounded, and a dropped
/// subscriber can never grow it.
pub(crate) const NOTIFICATION_CAPACITY: usize = 256;
/// A frame larger than this is a broken or hostile server, not an answer worth buffering.
pub(crate) const MAX_FRAME: usize = 64 * 1024 * 1024;
/// JSON-RPC "method not found". Used to answer server-to-client requests we do not implement, so the server
/// never waits on us.
pub(crate) const METHOD_NOT_FOUND: i64 = -32601;

/// One language server process, with request/response correlation and notification fan-out.
pub struct Client {
    child: Mutex<Option<Child>>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Result<Value>>>>>,
    notifications: broadcast::Sender<Value>,
    /// Cleared when the read loop ends, so a request after the server died fails immediately instead of
    /// blocking for its whole timeout on a pipe nobody is reading.
    alive: Arc<AtomicBool>,
    next_id: AtomicI64,
}

impl Client {
    /// Spawns `bin` with `root` as its working directory. Dies with the client (`kill_on_drop`), so a dropped
    /// session cannot leave a stray server behind.
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

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Protocol("language server has no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Protocol("language server has no stdout".into()))?;

        let stdin = Arc::new(Mutex::new(stdin));
        let pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Result<Value>>>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let (notifications, _) = broadcast::channel(NOTIFICATION_CAPACITY);
        let alive = Arc::new(AtomicBool::new(true));

        tokio::spawn(read_loop(
            BufReader::new(stdout),
            pending.clone(),
            notifications.clone(),
            stdin.clone(),
            alive.clone(),
        ));

        Ok(Self {
            child: Mutex::new(Some(child)),
            stdin,
            pending,
            notifications,
            alive,
            next_id: AtomicI64::new(1),
        })
    }

    /// Whether the server is still running. A dead session must be replaced, not reused.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Sends a request and waits for its response. Notifications that arrive meanwhile are queued for
    /// `subscribe`, never mistaken for the answer. A JSON-RPC `error` member becomes `Error::Protocol`.
    pub async fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        if !self.is_alive() {
            return Err(Error::Protocol(format!("{method}: the language server is not running")));
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        // Insert before writing: the answer can arrive before the write call returns.
        self.pending.lock().insert(id, tx);

        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if let Err(e) = write_frame(&mut *self.stdin.lock().await, &message).await {
            self.pending.lock().remove(&id);
            return Err(e);
        }

        let response = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(value))) => value,
            Ok(Ok(Err(e))) => return Err(e),
            // The read loop fails every pending request when it stops, so this is a genuine protocol surprise.
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
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        write_frame(&mut *self.stdin.lock().await, &message).await
    }

    /// Server-initiated messages, for progress and diagnostics.
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.notifications.subscribe()
    }

    /// `shutdown`/`exit`, then reap. Best-effort throughout: the app is quitting.
    pub async fn shutdown(&self) {
        let _ = self.request("shutdown", Value::Null, Duration::from_secs(2)).await;
        let _ = self.notify("exit", Value::Null).await;
        let mut guard = self.child.lock().await;
        if let Some(child) = guard.as_mut() {
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            let _ = child.start_kill();
        }
        *guard = None;
    }
}

/// Routes responses to their waiting caller and notifications to subscribers, answers server-to-client
/// requests with "method not found", and on exit fails everything still in flight — a caller must never hang
/// until its timeout because the child went away.
async fn read_loop<R, W>(
    mut reader: R,
    pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Result<Value>>>>>,
    notifications: broadcast::Sender<Value>,
    writer: Arc<Mutex<W>>,
    alive: Arc<AtomicBool>,
) where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    loop {
        match read_frame(&mut reader).await {
            Ok(Some(message)) => match message.get("id") {
                Some(id) if !id.is_null() => {
                    if message.get("method").is_some() {
                        // A server-to-client request (`window/workDoneProgress/create` and friends). We
                        // advertise none of those capabilities, so answer "method not found" rather than
                        // leaving the server waiting on a reply that will never come.
                        let reply = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": { "code": METHOD_NOT_FOUND, "message": "method not supported" }
                        });
                        let _ = write_frame(&mut *writer.lock().await, &reply).await;
                    } else if let Some(id) = id.as_i64() {
                        if let Some(sender) = pending.lock().remove(&id) {
                            let _ = sender.send(Ok(message));
                        }
                    }
                }
                _ => {
                    let _ = notifications.send(message);
                }
            },
            Ok(None) => break,
            // A frame we cannot parse means the stream is no longer trustworthy.
            Err(_) => break,
        }
    }

    alive.store(false, Ordering::SeqCst);
    // Dropping the waiters closes their channels, so an in-flight `request` reports the failure at once.
    pending.lock().clear();
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

    let Some(length) = content_length else {
        return Err(Error::Protocol("frame without Content-Length".into()));
    };
    if length > MAX_FRAME {
        return Err(Error::Protocol(format!("frame of {length} bytes is too large")));
    }

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;
    // Some servers prefix a byte-order mark; `serde_json` rejects it.
    let body = body.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&body);
    Ok(Some(serde_json::from_slice(body)?))
}

pub(crate) async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, message: &Value) -> Result<()> {
    let body = serde_json::to_vec(message)?;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    fn loop_args(
        reader: tokio::io::DuplexStream,
        pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Result<Value>>>>>,
        notifications: broadcast::Sender<Value>,
        writer: Arc<Mutex<tokio::io::DuplexStream>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(read_loop(
            BufReader::new(reader),
            pending,
            notifications,
            writer,
            Arc::new(AtomicBool::new(true)),
        ))
    }

    #[tokio::test]
    async fn frames_round_trip() {
        let (mut writer, reader) = duplex(4096);
        let sending = tokio::spawn(async move {
            write_frame(&mut writer, &json!({ "jsonrpc": "2.0", "id": 1, "result": { "ok": true } }))
                .await
                .unwrap();
            write_frame(&mut writer, &json!({ "jsonrpc": "2.0", "method": "$/progress" }))
                .await
                .unwrap();
        });

        let mut reader = BufReader::new(reader);
        let first = read_frame(&mut reader).await.unwrap().unwrap();
        assert_eq!(first["id"], 1);
        assert_eq!(first["result"]["ok"], true);
        let second = read_frame(&mut reader).await.unwrap().unwrap();
        assert_eq!(second["method"], "$/progress");
        sending.await.unwrap();
    }

    #[tokio::test]
    async fn extra_headers_and_odd_casing_are_tolerated() {
        let (mut writer, reader) = duplex(4096);
        let body = br#"{"id":7}"#;
        writer
            .write_all(
                format!(
                    "Content-Type: application/vscode-jsonrpc; charset=utf-8\r\ncontent-length: {}\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        writer.write_all(body).await.unwrap();

        let mut reader = BufReader::new(reader);
        let message = read_frame(&mut reader).await.unwrap().unwrap();
        assert_eq!(message["id"], 7);
    }

    #[tokio::test]
    async fn a_byte_order_mark_does_not_break_a_frame() {
        let (mut writer, reader) = duplex(4096);
        let body: Vec<u8> = vec![0xEF, 0xBB, 0xBF, b'{', b'"', b'i', b'd', b'"', b':', b'3', b'}'];
        writer
            .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
            .await
            .unwrap();
        writer.write_all(&body).await.unwrap();

        let mut reader = BufReader::new(reader);
        let message = read_frame(&mut reader).await.unwrap().unwrap();
        assert_eq!(message["id"], 3);
    }

    #[tokio::test]
    async fn oversized_frames_are_refused_before_reading_the_body() {
        let (mut writer, reader) = duplex(4096);
        writer
            .write_all(b"Content-Length: 999999999999\r\n\r\n")
            .await
            .unwrap();
        let mut reader = BufReader::new(reader);
        assert!(matches!(
            read_frame(&mut reader).await,
            Err(Error::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn eof_closes_in_flight_requests() {
        let (writer, reader) = duplex(64);
        let pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Result<Value>>>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().insert(1, tx);
        let (mutations, _) = duplex(64);
        let stdin = Arc::new(Mutex::new(mutations));
        let (notifications, _) = broadcast::channel(8);

        let reading = loop_args(reader, pending.clone(), notifications, stdin);
        drop(writer);
        reading.await.unwrap();

        // The waiter must observe a closed channel rather than waiting out its own timeout.
        assert!(rx.await.is_err());
        assert!(pending.lock().is_empty());
    }

    #[tokio::test]
    async fn notifications_reach_subscribers_and_unknown_ids_are_ignored() {
        let (mut writer, reader) = duplex(4096);
        let pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Result<Value>>>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let (notifications, _) = broadcast::channel(8);
        let mut subscriber = notifications.subscribe();
        let (mutations, _) = duplex(64);
        let stdin = Arc::new(Mutex::new(mutations));

        let reading = loop_args(reader, pending.clone(), notifications.clone(), stdin);

        write_frame(&mut writer, &json!({ "jsonrpc": "2.0", "id": 99, "result": null }))
            .await
            .unwrap();
        write_frame(&mut writer, &json!({ "jsonrpc": "2.0", "method": "window/logMessage" }))
            .await
            .unwrap();
        drop(writer);
        reading.await.unwrap();

        assert_eq!(subscriber.recv().await.unwrap()["method"], "window/logMessage");
        assert!(pending.lock().is_empty());
    }
}
