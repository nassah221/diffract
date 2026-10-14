use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(10);
const SKIP_VARS: &[&str] = &["PWD", "OLDPWD", "SHLVL", "_", "TERM", "TERM_PROGRAM", "PS1"];

/// GUI launches on macOS get a minimal environment; import the user's login-shell env so git/node/agents resolve.
pub fn load_login_shell_env() {
    if cfg!(windows) {
        return;
    }
    match read_shell_env() {
        Some(output) => {
            for entry in output.split('\0') {
                let Some((key, value)) = entry.split_once('=') else {
                    continue;
                };
                if key.is_empty() || SKIP_VARS.contains(&key) {
                    continue;
                }
                std::env::set_var(key, value);
            }
        }
        None => tracing::warn!("env_fix: could not read login shell environment"),
    }
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let current = std::env::var_os("PATH").unwrap_or_default();
    if let Some(path) = with_fallback_dirs(&current, home.as_deref()) {
        std::env::set_var("PATH", path);
    }
}

/// Install locations of Claude Code, Node and common version managers, appended to PATH when present, so the
/// agent still resolves when the login shell is slow, prints errors, or doesn't export them.
fn fallback_dirs(home: Option<&std::path::Path>) -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(home) = home {
        for rel in [
            ".local/bin",
            "go/bin",
            ".claude/local",
            ".volta/bin",
            ".bun/bin",
            ".npm-global/bin",
            ".asdf/shims",
            ".local/share/fnm/aliases/default/bin",
            ".fnm/aliases/default/bin",
            ".local/share/mise/shims",
        ] {
            dirs.push(home.join(rel));
        }
        if let Some(bin) = newest_nvm_node(&home.join(".nvm/versions/node")) {
            dirs.push(bin);
        }
    }
    for abs in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/homebrew/opt/node/bin"] {
        dirs.push(std::path::PathBuf::from(abs));
    }
    dirs
}

fn newest_nvm_node(versions: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut found: Vec<(Vec<u64>, std::path::PathBuf)> = std::fs::read_dir(versions)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let parts: Vec<u64> = name.trim_start_matches('v').split('.').filter_map(|p| p.parse().ok()).collect();
            let bin = entry.path().join("bin");
            if parts.is_empty() || !bin.is_dir() {
                return None;
            }
            Some((parts, bin))
        })
        .collect();
    found.sort();
    found.pop().map(|(_, bin)| bin)
}

fn with_fallback_dirs(current: &std::ffi::OsStr, home: Option<&std::path::Path>) -> Option<std::ffi::OsString> {
    let mut paths: Vec<std::path::PathBuf> = std::env::split_paths(current).collect();
    let mut changed = false;
    for dir in fallback_dirs(home) {
        if dir.is_dir() && !paths.contains(&dir) {
            paths.push(dir);
            changed = true;
        }
    }
    if !changed {
        return None;
    }
    std::env::join_paths(paths).ok()
}

fn read_shell_env() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut child = Command::new(shell)
        .args(["-ilc", "env -0"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() > TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return None,
        }
    }
    let buf = reader.join().ok()?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    if !text.contains("PATH=") {
        return None;
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_existing_fallback_dirs_once() {
        let home = std::env::temp_dir().join(format!("diffity-envfix-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        std::fs::create_dir_all(home.join(".nvm/versions/node/v18.1.0/bin")).unwrap();
        std::fs::create_dir_all(home.join(".nvm/versions/node/v22.4.0/bin")).unwrap();
        let path = with_fallback_dirs(std::ffi::OsStr::new("/usr/bin"), Some(&home)).unwrap();
        let parts: Vec<std::path::PathBuf> = std::env::split_paths(&path).collect();
        assert_eq!(parts[0], std::path::PathBuf::from("/usr/bin"));
        assert!(parts.contains(&home.join(".local/bin")));
        assert!(parts.contains(&home.join(".nvm/versions/node/v22.4.0/bin")));
        assert!(!parts.contains(&home.join(".nvm/versions/node/v18.1.0/bin")));
        assert!(!parts.contains(&home.join(".volta/bin")));
        let again = with_fallback_dirs(&path, Some(&home));
        assert!(again.is_none());
        std::fs::remove_dir_all(&home).ok();
    }
}
