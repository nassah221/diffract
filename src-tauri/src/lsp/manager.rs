//! Per-repo language server sessions: detection, get-or-start, idle eviction, shutdown.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::Mutex as AsyncMutex;

use crate::agents::detect::{find_in_path, is_executable};
use crate::lsp::session::Session;
use crate::lsp::{canonical, Error, Result};

const SERVER: &str = "gopls";
/// A session nobody has touched for this long is stopped when the next request arrives. The app has no
/// repo-switch event on this side and supports several windows, so eviction has to be time-based.
const IDLE_TTL: Duration = Duration::from_secs(600);

type Slot = Arc<AsyncMutex<Option<Arc<Session>>>>;

pub struct LspManager {
    slots: Mutex<HashMap<PathBuf, Slot>>,
}

impl Default for LspManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LspManager {
    pub fn new() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
        }
    }

    /// The session for `root`, started on first use. Concurrent callers for the same root share one server:
    /// the start happens under that repo's slot lock.
    pub async fn session(&self, root: &Path) -> Result<Arc<Session>> {
        let root = canonical(root);

        for stale in self.idle_roots() {
            self.stop(&stale).await;
        }

        let slot = {
            let mut slots = self.slots.lock();
            slots.entry(root.clone()).or_default().clone()
        };
        let mut guard = slot.lock().await;
        if let Some(existing) = guard.as_ref() {
            if existing.is_alive() {
                existing.touch();
                return Ok(existing.clone());
            }
            // The server exited under us. Drop it and start a fresh one below rather than handing out a
            // corpse for the rest of the session: idle eviction cannot rescue this, because every failed
            // click would touch it and reset the timer.
            *guard = None;
        }

        let bin = detect_server().ok_or_else(|| Error::ServerMissing {
            searched: searched_description(),
        })?;
        let session = Arc::new(Session::start(&bin, &root).await?);
        *guard = Some(session.clone());
        Ok(session)
    }

    /// Stops every server. Called from the app's exit hook beside `AgentManager::shutdown`.
    pub async fn shutdown(&self) {
        for root in self.roots() {
            self.stop(&root).await;
        }
    }

    pub fn roots(&self) -> Vec<PathBuf> {
        self.slots.lock().keys().cloned().collect()
    }

    async fn stop(&self, root: &Path) {
        let slot = self.slots.lock().remove(root);
        if let Some(slot) = slot {
            if let Some(session) = slot.lock().await.take() {
                session.shutdown().await;
            }
        }
    }

    /// Sessions past the idle TTL that nobody is holding. `try_lock` failing means a request is in flight,
    /// and a strong count above one means a caller still has the session, so both are left alone.
    fn idle_roots(&self) -> Vec<PathBuf> {
        let entries: Vec<(PathBuf, Slot)> = self
            .slots
            .lock()
            .iter()
            .map(|(root, slot)| (root.clone(), slot.clone()))
            .collect();

        entries
            .into_iter()
            .filter(|(_, slot)| {
                slot.try_lock()
                    .map(|guard| match guard.as_ref() {
                        Some(session) => session.idle_for() > IDLE_TTL && Arc::strong_count(session) == 1,
                        None => false,
                    })
                    .unwrap_or(false)
            })
            .map(|(root, _)| root)
            .collect()
    }
}

/// Looks where `go install` and Homebrew actually put gopls, then falls back to PATH.
///
/// PATH alone is not enough: a GUI launch gets the login shell's PATH (`env_fix::load_login_shell_env`), which
/// usually predates any Go tooling, and `~/go/bin` is not in its fallback list.
pub fn detect_server() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(gobin) = std::env::var_os("GOBIN") {
        candidates.push(PathBuf::from(gobin).join(SERVER));
    }
    if let Some(gopath) = std::env::var_os("GOPATH") {
        for dir in std::env::split_paths(&gopath) {
            candidates.push(dir.join("bin").join(SERVER));
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join("go/bin").join(SERVER));
    }
    if let Some(found) = candidates.into_iter().find(|path| is_executable(path)) {
        return Some(found);
    }

    find_in_path(SERVER)
}

fn searched_description() -> String {
    let mut places = vec![format!("~/.local/bin/{SERVER}"), "PATH".to_string()];
    if std::env::var_os("GOBIN").is_some() {
        places.insert(0, format!("$GOBIN/{SERVER}"));
    }
    if std::env::var_os("GOPATH").is_some() {
        places.insert(0, format!("$GOPATH/bin/{SERVER}"));
    }
    places.insert(0, format!("~/go/bin/{SERVER}"));
    places.join(", ")
}
