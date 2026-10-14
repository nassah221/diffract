//! LSP client module root: types, errors, constants shared by client, session, and manager.

pub mod client;
pub mod manager;
pub mod session;

pub use manager::LspManager;

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: String,
    pub inside_repo: bool,
    pub line: u32,
    pub start_character: u32,
    pub end_character: u32,
}

/// What a symbol is, as a language server reports it for a position: typically a signature and its doc
/// comment. `kind` is what the server sent (`markdown` for gopls), so the UI can decide how to render it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hover {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("server missing: {searched}")]
    ServerMissing { searched: String },
    #[error("not eligible: {reason}")]
    NotEligible { reason: String },
    #[error("the line changed since the diff was loaded")]
    LineMoved,
    #[error("{method} timed out")]
    Timeout { method: String },
    #[error("lsp protocol error: {0}")]
    Protocol(String),
    #[error("json error")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ServerMissing { .. } => "lsp_server_missing",
            Self::NotEligible { .. } => "lsp_not_eligible",
            Self::LineMoved => "lsp_line_moved",
            Self::Timeout { .. } => "lsp_timeout",
            Self::Protocol(_) => "lsp_protocol",
            Self::Json(_) => "json",
            Self::Io(_) => "io",
        }
    }
}

/// Turn a `file://` URI into a path, percent-decoding the escapes gopls emits for spaces and unicode.
pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                    continue;
                }
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).ok().map(PathBuf::from)
}

/// `file://` URI for a path, percent-encoding what it must. A repo path with a space or a non-ASCII
/// character is common enough on macOS that the encoding has to be real.
pub fn uri_from_path(path: &Path) -> String {
    let mut uri = String::from("file://");
    for &byte in path.to_string_lossy().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(byte as char)
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

pub(crate) fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
