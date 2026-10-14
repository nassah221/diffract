//! LSP commands: the Tauri commands that wrap `LspManager` and the eligibility gate.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::core::diff::Source;
use crate::core::AppError;
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

/// Whether the language server can see this side of the diff on disk.
///
/// `Source::WorkTree` is on disk. `Source::Commit(sha)` only matches when `sha` is the repo's current HEAD —
/// which is exactly what a pull request diffs to (`origin/<base>...HEAD` resolves through the range plan), so
/// PR review stays eligible while a `main..old-sha` range does not.
///
/// The comparison is against the repository's HEAD (`git::head_sha`), never `DiffPlan::resolved.head_sha`:
/// the range plan sets that to the range's right-hand sha, so it would always equal itself.
fn is_eligible(side: &Source, head: &Option<String>) -> bool {
    match side {
        Source::WorkTree => true,
        Source::Commit(sha) => head.as_ref().is_some_and(|h| h == sha),
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
        Source::Commit(sha) if !head.as_ref().is_some_and(|h| h == sha) => {
            "this view shows a commit that is not the current checkout".to_string()
        }
        Source::EmptyTree => "this view shows an empty tree".to_string(),
        _ => "this view is not the working tree".to_string(),
    }
}

/// Reads one line of a repo-relative file as the diff would show it, for the echo guard.
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
    let (source, head) = crate::commands::repo::blocking(move || {
        let plan = crate::core::diff::plan(&repo, &r#ref)?;
        let head = crate::core::git::head_sha(&repo)?;
        Ok::<(Source, Option<String>), AppError>((plan.new, head))
    })
    .await?;

    let eligible = is_eligible(&source, &head);
    Ok(Eligibility {
        eligible,
        side: eligible.then(|| side_name(&source)),
        reason: (!eligible).then(|| ineligibility_reason(&source, &head)),
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
        return Err(AppError::invalid("lines are 1-based"));
    }
    if !path.ends_with(".go") {
        return Err(crate::lsp::Error::NotEligible {
            reason: "only Go files are supported so far".to_string(),
        }
        .into());
    }

    // One blocking hop for everything that touches git or the filesystem, and the staleness check before any
    // position is trusted: the diff can be showing an index blob or a commit while gopls reads the disk.
    let repo = PathBuf::from(repo_path);
    let relative = path.clone();
    let expected = line_text.clone();
    let (repo, absolute) = crate::commands::repo::blocking(move || {
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
        Ok::<(PathBuf, PathBuf), AppError>((repo.clone(), repo.join(&relative)))
    })
    .await?;

    let session = state.lsp.session(&repo).await?;
    // Keep the file the server will resolve against current: the echo guard above already proved this line is
    // what is on disk, and pushing it covers the case where the file changed since the last click.
    session.ensure_open(&absolute).await?;
    // The frontend sends 1-based lines; LSP positions are 0-based.
    Ok(session.definition(&absolute, line - 1, character).await?)
}

#[tauri::command]
pub async fn lsp_status(state: State<'_, AppState>) -> Result<LspStatus, AppError> {
    let found = crate::lsp::manager::detect_server();
    Ok(LspStatus {
        server_found: found.is_some(),
        server_path: found.map(|path| path.to_string_lossy().into_owned()),
        roots: state
            .lsp
            .roots()
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::diff;

    /// A throwaway repository with two commits, `origin/main` pointing at the first, so the PR-shaped ref
    /// `origin/main...HEAD` resolves the way a checked-out pull request does.
    fn repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("diffity-lsp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cmd")).unwrap();
        std::fs::write(
            dir.join("cmd/main.go"),
            "package main\n\nfunc main() { other() }\n\nfunc other() {}\n",
        )
        .unwrap();

        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .current_dir(&dir)
                .args(args)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "T"]);
        git(&["add", "."]);
        git(&["commit", "-qm", "base"]);
        git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        std::fs::write(dir.join("cmd/extra.go"), "package main\n\nfunc extra() {}\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "work"]);
        dir
    }

    /// The rule that keeps PR review working: a side is eligible when the server can see it on disk.
    #[test]
    fn only_workspace_backed_sides_are_eligible() {
        let head = Some("abc123".to_string());

        assert!(is_eligible(&Source::WorkTree, &head));
        // `origin/<base>...HEAD` — what every checked-out pull request diffs to.
        assert!(is_eligible(&Source::Commit("abc123".into()), &head));
        // A range that ends anywhere else is a different tree than the checkout.
        assert!(!is_eligible(&Source::Commit("def456".into()), &head));
        // The index is not the working tree, and there is nothing to compare against without HEAD.
        assert!(!is_eligible(&Source::Index, &head));
        assert!(!is_eligible(&Source::EmptyTree, &head));
        assert!(!is_eligible(&Source::Commit("abc123".into()), &None));
    }

    /// The same rule, through the real resolver, on a real repository.
    #[test]
    fn real_refs_resolve_to_the_expected_side() {
        let dir = repo("refs");
        let head = crate::core::git::head_sha(&dir).unwrap().expect("head");
        let head_opt = Some(head.clone());

        let working = diff::plan(&dir, "work").unwrap();
        assert!(is_eligible(&working.new, &head_opt), "work is the working tree");

        let staged = diff::plan(&dir, "staged").unwrap();
        assert!(!is_eligible(&staged.new, &head_opt), "the index is not on disk");

        let pr = diff::plan(&dir, "origin/main...HEAD").unwrap();
        assert!(
            is_eligible(&pr.new, &head_opt),
            "a checked-out PR diffs HEAD against its base and must stay eligible"
        );

        // A range ending at an older commit: `new` is that commit, not the checkout.
        let older = std::process::Command::new("git")
            .current_dir(&dir)
            .args(["rev-parse", "HEAD~1"])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap();
        assert!(!older.is_empty());
        let range = diff::plan(&dir, &format!("{head}..{older}")).unwrap();
        assert!(
            !is_eligible(&range.new, &head_opt),
            "a range ending at an older commit is not the checkout"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The echo guard: the rendered line must still be the line on disk before a position is trusted.
    #[test]
    fn the_echo_guard_rejects_a_moved_line() {
        let dir = repo("echo");
        assert_eq!(disk_line(&dir, "cmd/main.go", 3).unwrap(), "func main() { other() }");
        assert_ne!(disk_line(&dir, "cmd/main.go", 3).unwrap(), "func main() { renamed() }");
        assert!(disk_line(&dir, "cmd/main.go", 99).is_err());
        assert!(disk_line(&dir, "cmd/missing.go", 1).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
