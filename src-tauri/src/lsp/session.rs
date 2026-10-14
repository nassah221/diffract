//! One language server session for one repository root.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::Mutex as AsyncMutex;

use crate::lsp::client::Client;
use crate::lsp::{path_from_uri, uri_from_path, Error, Location, Result};

/// gopls answers `initialize` immediately (measured 0.03s) and loads packages lazily, so this only covers a
/// server that never starts talking.
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(10);
/// Generous on purpose. The load this waits on is the first query against a package that is not built yet,
/// which can include fetching its modules: a cold CLI run on a 390k-line repo measured 114s. A warm query is
/// measured at 5-7ms, so this ceiling is only ever reached by genuine first-run work.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// What the server has been told about a file, so a change on disk can be pushed instead of ignored.
struct Opened {
    modified: Option<SystemTime>,
    version: i64,
}

/// A `gopls serve` child plus the document state the UI has pushed into it.
pub struct Session {
    client: Client,
    root: PathBuf,
    /// Async because the guard is held across the notification that follows the check.
    opened: AsyncMutex<HashMap<PathBuf, Opened>>,
    last_used: Mutex<Instant>,
}

impl Session {
    pub async fn start(bin: &Path, root: &Path) -> Result<Self> {
        // gopls answers with resolved paths (`/tmp` comes back as `/private/tmp`), so the root must be
        // resolved too or every in-repo target looks like it lives outside the repo.
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let client = Client::spawn(bin, &root).await?;

        let root_uri = uri_from_path(&root);
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".to_string());
        let params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "workspaceFolders": [{ "uri": root_uri, "name": name }],
            "capabilities": {
                // gopls defaults to UTF-16, which is what JS string offsets already are; state it so a
                // future gopls that negotiates UTF-8 cannot silently shift our columns.
                "general": { "positionEncodings": ["utf-16"] },
                "textDocument": { "definition": { "linkSupport": true } },
                "workspace": { "workspaceFolders": true },
            },
        });
        client.request("initialize", params, INITIALIZE_TIMEOUT).await?;
        client.notify("initialized", json!({})).await?;

        Ok(Self {
            client,
            root,
            opened: AsyncMutex::new(HashMap::new()),
            last_used: Mutex::new(Instant::now()),
        })
    }

    /// The canonical repo root this session was started for.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the server is still running. A dead session must be replaced, not reused.
    pub fn is_alive(&self) -> bool {
        self.client.is_alive()
    }

    pub fn touch(&self) {
        *self.last_used.lock() = Instant::now();
    }

    pub fn idle_for(&self) -> Duration {
        self.last_used.lock().elapsed()
    }

    /// Tells the server what the file on disk looks like, and keeps telling it: an open document's contents
    /// are authoritative for gopls, so a file that changed under us (the user edited it, or this app reverted a
    /// hunk) has to be pushed as a `didChange` or every later answer comes from the stale text.
    pub async fn ensure_open(&self, absolute: &Path) -> Result<()> {
        let resolved = std::fs::canonicalize(absolute).unwrap_or_else(|_| absolute.to_path_buf());
        let path = resolved.clone();
        // Stat and read together so the mtime cannot drift between the two.
        let (text, modified) = tokio::task::spawn_blocking(move || {
            let modified = std::fs::metadata(&path).ok().and_then(|meta| meta.modified().ok());
            std::fs::read_to_string(&path).map(|text| (text, modified))
        })
        .await
        .map_err(|e| Error::Protocol(format!("read failed: {e}")))??;

        let mut opened = self.opened.lock().await;
        if let Some(previous) = opened.get(&resolved) {
            // An unknown mtime (a filesystem that does not report one) can never be matched, so the document
            // is re-sent rather than assumed unchanged.
            if previous.modified.is_some() && previous.modified == modified {
                return Ok(());
            }
        }

        let version = match opened.get(&resolved) {
            Some(previous) => {
                let version = previous.version + 1;
                self.client
                    .notify(
                        "textDocument/didChange",
                        json!({
                            "textDocument": { "uri": uri_from_path(&resolved), "version": version },
                            "contentChanges": [{ "text": text }],
                        }),
                    )
                    .await?;
                version
            }
            None => {
                self.client
                    .notify(
                        "textDocument/didOpen",
                        json!({
                            "textDocument": {
                                "uri": uri_from_path(&resolved),
                                "languageId": "go",
                                "version": 1,
                                "text": text,
                            }
                        }),
                    )
                    .await?;
                1
            }
        };
        opened.insert(resolved, Opened { modified, version });
        Ok(())
    }

    /// `line`/`character` are 0-based UTF-16, the units LSP uses and the DOM already gives us.
    pub async fn definition(&self, absolute: &Path, line: u32, character: u32) -> Result<Vec<Location>> {
        let resolved = std::fs::canonicalize(absolute).unwrap_or_else(|_| absolute.to_path_buf());
        let result = self
            .client
            .request(
                "textDocument/definition",
                json!({
                    "textDocument": { "uri": uri_from_path(&resolved) },
                    "position": { "line": line, "character": character },
                }),
                REQUEST_TIMEOUT,
            )
            .await?;
        Ok(locations_from_lsp(&self.root, &result))
    }

    pub async fn shutdown(&self) {
        self.client.shutdown().await;
    }
}

/// `definition` may answer with one `Location`, an array of them, or `LocationLink`s (a server advertising
/// `linkSupport` gets asked for those) — and `null` when there is nothing at that position.
fn locations_from_lsp(root: &Path, value: &Value) -> Vec<Location> {
    let entries: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![value],
        _ => Vec::new(),
    };
    entries
        .iter()
        .filter_map(|entry| location_from_lsp(root, entry))
        .collect()
}

fn location_from_lsp(root: &Path, value: &Value) -> Option<Location> {
    let uri = value.get("targetUri").or_else(|| value.get("uri"))?.as_str()?;
    let range = value
        .get("targetSelectionRange")
        .or_else(|| value.get("targetRange"))
        .or_else(|| value.get("range"))?;
    let start = range.get("start")?;

    let path = path_from_uri(uri)?;
    // Resolve before comparing: a symlinked root (macOS `/tmp`, `/var`) would otherwise never match.
    let resolved = std::fs::canonicalize(&path).unwrap_or(path);
    let inside_repo = resolved.starts_with(root) && resolved != root;
    let shown = if inside_repo {
        resolved
            .strip_prefix(root)
            .ok()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| resolved.clone())
    } else {
        resolved.clone()
    };

    let line = start.get("line")?.as_u64()? as u32;
    let start_character = start.get("character")?.as_u64()? as u32;
    let end_character = range
        .get("end")
        .and_then(|end| end.get("character"))
        .and_then(Value::as_u64)
        .map(|c| c as u32)
        .unwrap_or(start_character);

    Some(Location {
        path: shown.to_string_lossy().into_owned(),
        inside_repo,
        line: line + 1,
        start_character,
        end_character,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_survive_the_uri_round_trip() {
        for path in [
            "/tmp/repo/main.go",
            "/tmp/my project/a b.go",
            "/tmp/ünïcode/đ.go",
            "/tmp/hash#and?query.go",
        ] {
            let uri = uri_from_path(Path::new(path));
            assert!(uri.starts_with("file:///"), "{uri}");
            assert_eq!(path_from_uri(&uri).unwrap(), PathBuf::from(path), "{uri}");
        }
    }

    #[test]
    fn locations_are_read_from_both_shapes() {
        let root = std::path::Path::new("/repo");
        let plain = json!([{
            "uri": "file:///repo/pkg/a.go",
            "range": { "start": { "line": 9, "character": 4 }, "end": { "line": 9, "character": 12 } }
        }]);
        let locations = locations_from_lsp(root, &plain);
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].line, 10, "LSP lines are 0-based, the UI is 1-based");
        assert_eq!(locations[0].start_character, 4);
        assert_eq!(locations[0].end_character, 12);
        assert_eq!(locations[0].path, "pkg/a.go");
        assert!(locations[0].inside_repo);

        let link = json!([{
            "targetUri": "file:///repo/b.go",
            "targetRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 40, "character": 1 } },
            "targetSelectionRange": { "start": { "line": 3, "character": 5 }, "end": { "line": 3, "character": 9 } }
        }]);
        let locations = locations_from_lsp(root, &link);
        // The selection range (the identifier), not the whole declaration body.
        assert_eq!(locations[0].line, 4);
        assert_eq!(locations[0].start_character, 5);
        assert_eq!(locations[0].end_character, 9);
    }

    #[test]
    fn nothing_found_is_an_empty_list() {
        let root = std::path::Path::new("/repo");
        assert!(locations_from_lsp(root, &Value::Null).is_empty());
        assert!(locations_from_lsp(root, &json!({ "unexpected": true })).is_empty());
    }

    #[test]
    fn targets_outside_the_repo_keep_their_absolute_path() {
        let root = std::path::Path::new("/repo");
        let outside = json!([{
            "uri": "file:///Users/someone/go/pkg/mod/example.com/lib@v1/x.go",
            "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 3 } }
        }]);
        let locations = locations_from_lsp(root, &outside);
        assert_eq!(locations.len(), 1);
        assert!(!locations[0].inside_repo);
        assert!(locations[0].path.starts_with('/'), "{}", locations[0].path);
    }
}
