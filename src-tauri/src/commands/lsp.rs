//! LSP commands: the Tauri commands that wrap `LspManager` and the eligibility gate.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::core::diff::{self, Source};
use crate::core::{git, AppError};
use crate::lsp::{Location, Result};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Eligibility {
    pub eligible: bool,
    pub side: Option<&'static str>,
    pub reason: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LspStatus {
    pub server_found: bool,
    pub server_path: Option<String>,
    pub roots: Vec<String>,
}

impl From<crate::lsp::Error> for AppError {
    fn from(e: crate::lsp::Error) -> Self {
        AppError::new(e.code(), e.to_string())
    }
}

fn is_eligible(side: &Source, head: &Option<String>) -> bool {
    match side {
        Source::WorkTree => true,
        Source::Commit(sha) => head.as_ref().map(|h| h.as_str() == sha.as_str()).unwrap_or(false),
        Source::Index | Source::EmptyTree => false,
    }
}

fn side_name(source: &Source) -> &'static str {
    match source {
        Source::WorkTree | Source::Commit(_) => "new",
        Source::Index | Source::EmptyTree => "old",
    }
}

fn ineligibility_reason(source: &Source, head: &Option<String>) -> String {
    match source {
        Source::Index => "this view shows the staged index, not the working tree".to_string(),
        Source::Commit(sha) if head.as_deref() != Some(sha.as_str()) => {
            "this view shows a commit that is not the current checkout".to_string()
        }
        Source::EmptyTree => "this view shows an empty tree".to_string(),
        _ => "this view is not the working tree".to_string(),
    }
}

/// Reads one line of a repo-relative file as git would show it, for the echo guard.
fn disk_line(repo: &Path, relative: &str, line: u32) -> Result<String, AppError> {
    let path = repo.join(relative);
    let text = std::fs::read_to_string(&path)?;
    text.lines()
        .nth(line as usize - 1)
        .map(|found| found.trim_end_matches('\r').to_string())
        .ok_or_else(|| AppError::invalid(format!("the file has no line {line}")))
}

#[tauri::command]
pub async fn lsp_eligible(repo_path: String, r#ref: String) -> Result<Eligibility, AppError> {
    let repo = PathBuf::from(repo_path);
    let ref_str = r#ref.clone();
    let (side_src, head) = crate::commands::repo::blocking(move || {
        let plan = crate::core::diff::plan(&repo, &r#ref)?;
        let head = crate::core::git::head_sha(&repo)?;
        Ok::<(Source, Option<String>), AppError>((plan.new, head))
    })
    .await?;

    let eligible = is_eligible(&side_src, &head);
    Ok(Eligibility {
        eligible,
        side: eligible.then_some(side_name(&side_src)),
        reason: (!eligible).then(|| ineligibility_reason(&side_src, &head)),
    })
}

#[tauri::command]
pub async fn lsp_definition(
    state: State<'_, AppState>,
    repo_path: String,
    r#ref: String,
    path: String,
    side: String,
    line: u32,
    character: u32,
    line_text: String,
) -> Result<Vec<Location>, AppError> {
    if side != "new" {
        return Err(crate::lsp::Error::NotEligible {
            reason: "only the new side of a diff is on disk".to_string(),
        }
        .into());
    }
    if line == 0 {
        return Err(AppError::invalid("line must be 1-based"));
    }
    if !path.ends_with(".go") {
        return Err(crate::lsp::Error::NotEligible {
            reason: "only Go files are supported so far".to_string(),
        }
        .into());
    }

    let repo = PathBuf::from(repo_path);
    let ref_str = r#ref.clone();
    let relative = path.clone();
    let expected = line_text.clone();
    // Full path for gopls; relative path still needed for disk checks.
    let full_path = repo.join(&relative);

    let _ = crate::commands::repo::blocking(move || {
        let plan = crate::core::diff::plan(&repo, &r#ref)?;
        let head = crate::core::git::head_sha(&repo)?;
        if !is_eligible(&plan.new, &head) {
            return Err(crate::lsp::Error::NotEligible {
                reason: ineligibility_reason(&plan.new, &head),
            }
            .into());
        }
        let found = disk_line(&repo, &relative, line)?;
        if found != expected.trim_end_matches('\r') {
            return Err(crate::lsp::Error::LineMoved.into());
        }
        Ok::<(), AppError>(())
    })
    .await?;

    let session = state.lsp.session(&repo).await?;
    // Keep the file story current on disk: revert-file, an edit, etc. makes this push independent of
    // whatever the diff previously displayed, provided the echo guard above already accepted the line.
    session.ensure_open(&full_path).await?;
    Ok(session.definition(&full_path, line, character).await?)
}

#[tauri::command]
pub async fn lsp_status(state: State<'_, AppState>) -> Result<LspStatus, AppError> {
    let manager = state.lsp.clone();
    let roots = manager.roots().await.iter().map(|root| root.to_string_lossy().into_owned()).collect();

    let found = {
        let server_path = manager.detect_server();
        match server_path {
            Ok(p) => Some(p.to_string_lossy().into_owned()),
            Err(_) => None,
        }
    };

    Ok(LspStatus {
        server_found: found.is_some(),
        server_path: found,
        roots,
    })
}
