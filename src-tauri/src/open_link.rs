//! `diffract://open?path=…` links, sent by the `diffract` command (src/bin/diffity-cli) and agents. Any app or web
//! page can fire one, so a link only opens, navigates or checks out a PR (behind the usual dirty-tree guard): it
//! never edits, comments or posts anything.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, Runtime, State, Url};

pub const OPEN_REQUESTS_EVENT: &str = "open-requests";

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenRequest {
    /// Absent for a pull request link run outside a repository; the app then looks for a local clone.
    pub path: Option<String>,
    pub view: Option<String>,
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    pub file: Option<String>,
    pub pr: Option<String>,
    pub new_window: bool,
}

/// Links that arrived before a window could take them (e.g. the one that launched the app).
#[derive(Default)]
pub struct PendingOpens(Mutex<Vec<OpenRequest>>);

fn is_safe_ref(value: &str) -> bool {
    !value.starts_with('-') && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn is_pr(value: &str) -> bool {
    let is_number = !value.is_empty() && value.chars().all(|c| c.is_ascii_digit());
    is_number || value.starts_with("https://github.com/")
}

pub fn parse(url: &Url) -> Option<OpenRequest> {
    let scheme_ok = url.scheme() == "diffract" || url.scheme() == "diffract-dev";
    if !scheme_ok || url.host_str() != Some("open") {
        return None;
    }
    let param = |key: &str| {
        url.query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let path = param("path").filter(|p| p.starts_with('/'));
    let view = param("view").filter(|v| v == "files" || v == "diff");
    let git_ref = param("ref").filter(|r| is_safe_ref(r));
    let file = param("file").filter(|f| !f.starts_with('/') && !f.split('/').any(|part| part == ".."));
    let pr = param("pr").filter(|p| is_pr(p));
    if path.is_none() && !pr.as_deref().is_some_and(|p| p.starts_with("https://")) {
        return None;
    }
    let new_window = param("window").as_deref() == Some("new");
    Some(OpenRequest { path, view, git_ref, file, pr, new_window })
}

/// The main window takes links; once it is closed, whichever repository window is left does.
fn target_window<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::WebviewWindow<R>> {
    app.get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next())
}

pub fn handle_urls<R: Runtime>(app: &AppHandle<R>, urls: Vec<Url>) {
    let requests: Vec<OpenRequest> = urls.iter().filter_map(parse).collect();
    if requests.is_empty() {
        tracing::warn!("ignored diffract link(s): {urls:?}");
        return;
    }
    app.state::<PendingOpens>().0.lock().unwrap_or_else(|e| e.into_inner()).extend(requests);
    let Some(window) = target_window(app) else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.set_focus();
    let label = window.label().to_string();
    if let Err(e) = app.emit_to(EventTarget::webview_window(label), OPEN_REQUESTS_EVENT, ()) {
        tracing::warn!("could not send {OPEN_REQUESTS_EVENT}: {e}");
    }
}

#[tauri::command]
pub fn take_open_requests(pending: State<'_, PendingOpens>) -> Vec<OpenRequest> {
    std::mem::take(&mut *pending.0.lock().unwrap_or_else(|e| e.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(link: &str) -> Option<OpenRequest> {
        parse(&Url::parse(link).unwrap())
    }

    #[test]
    fn parses_a_full_link() {
        let request = open("diffract://open?path=/Users/me/my%20repo&ref=main...HEAD&file=src/a.ts&window=new").unwrap();
        assert_eq!(request.path.as_deref(), Some("/Users/me/my repo"));
        assert_eq!(request.git_ref.as_deref(), Some("main...HEAD"));
        assert_eq!(request.file.as_deref(), Some("src/a.ts"));
        assert!(request.new_window);
    }

    #[test]
    fn needs_an_absolute_path() {
        assert!(open("diffract-dev://open?path=/a").is_some());
        assert!(open("other://open?path=/a").is_none());
        assert!(open("diffract://open").is_none());
        assert!(open("diffract://open?path=relative").is_none());
        assert!(open("diffract://other?path=/a").is_none());
    }

    #[test]
    fn drops_unsafe_values() {
        let request = open("diffract://open?path=/a&ref=--output%3D/tmp/x&file=../../etc/passwd&pr=1;rm&view=x").unwrap();
        assert_eq!(request.git_ref, None);
        assert_eq!(request.file, None);
        assert_eq!(request.pr, None);
        assert_eq!(request.view, None);
    }

    #[test]
    fn accepts_pull_requests() {
        assert_eq!(open("diffract://open?path=/a&pr=42").unwrap().pr.as_deref(), Some("42"));
        let url = "https://github.com/o/r/pull/7";
        assert_eq!(open(&format!("diffract://open?path=/a&pr={url}")).unwrap().pr.as_deref(), Some(url));
        let without_repo = open(&format!("diffract://open?pr={url}")).unwrap();
        assert_eq!(without_repo.path, None);
        assert_eq!(without_repo.pr.as_deref(), Some(url));
        assert!(open("diffract://open?pr=7").is_none());
    }
}
