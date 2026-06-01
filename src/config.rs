use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct ConfigFile {
    repos: Vec<String>,
}

/// Expand a leading `~` using $HOME. No-op if HOME unset or no leading tilde.
fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(s)
}

/// Default config path: $XDG_CONFIG_HOME/wip/repos.toml or ~/.config/wip/repos.toml
pub fn default_config_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("wip/repos.toml");
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config/wip/repos.toml")
}

pub fn load_from_file(path: &Path) -> Result<Vec<PathBuf>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("cannot read config {}: {e}", path.display()))?;
    let cfg: ConfigFile =
        toml::from_str(&content).map_err(|e| format!("bad config {}: {e}", path.display()))?;
    Ok(cfg.repos.iter().map(|s| expand_tilde(s)).collect())
}

/// Immediate subdirectories of `root` that contain a `.git` entry.
pub fn scan_root(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for e in entries.flatten() {
            let p = e.path();
            if p.join(".git").exists() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_parses_repo_list() {
        let d = TempDir::new().unwrap();
        let cfg = d.path().join("repos.toml");
        std::fs::write(&cfg, "repos = [\"/a/b\", \"/c/d\"]\n").unwrap();
        let repos = load_from_file(&cfg).unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0], PathBuf::from("/a/b"));
    }

    #[test]
    fn load_errors_on_missing_file() {
        let err = load_from_file(Path::new("/no/such/repos.toml")).unwrap_err();
        assert!(err.contains("cannot read config"));
    }

    #[test]
    fn scan_root_finds_git_dirs() {
        let d = TempDir::new().unwrap();
        let repo = d.path().join("proj1");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(d.path().join("not-a-repo")).unwrap();
        let found = scan_root(d.path());
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("proj1"));
    }
}
