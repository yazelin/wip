use std::path::Path;
use std::process::Command;

fn run_git(repo: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").current_dir(repo).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn branch(repo: &Path) -> String {
    run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "?".into())
}

pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub rel_time: String,
    pub ts: i64,
}

pub fn last_commit(repo: &Path) -> Option<CommitInfo> {
    // %x1f = unit separator, safe field delimiter
    let raw = run_git(repo, &["log", "-1", "--format=%h%x1f%s%x1f%cr%x1f%ct"])?;
    let parts: Vec<&str> = raw.split('\u{1f}').collect();
    if parts.len() != 4 {
        return None;
    }
    Some(CommitInfo {
        sha: parts[0].to_string(),
        message: parts[1].to_string(),
        rel_time: parts[2].to_string(),
        ts: parts[3].parse().unwrap_or(0),
    })
}

pub fn dirty_count(repo: &Path) -> u32 {
    match run_git(repo, &["status", "--porcelain"]) {
        Some(s) if !s.is_empty() => s.lines().count() as u32,
        _ => 0,
    }
}

pub fn unpushed_count(repo: &Path) -> u32 {
    // @{u} fails when no upstream tracked -> treat as 0
    run_git(repo, &["rev-list", "--count", "@{u}..HEAD"])
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        let run = |args: &[&str]| {
            let s = Command::new("git").current_dir(p).args(args).output().unwrap();
            assert!(s.status.success(), "git {:?} failed", args);
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.email", "t@t.com"]);
        run(&["config", "user.name", "t"]);
        std::fs::write(p.join("a.txt"), "hi").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "first commit"]);
        dir
    }

    #[test]
    fn branch_reports_main() {
        let d = init_repo();
        assert_eq!(branch(d.path()), "main");
    }

    #[test]
    fn last_commit_parsed() {
        let d = init_repo();
        let c = last_commit(d.path()).expect("commit");
        assert_eq!(c.message, "first commit");
        assert!(!c.sha.is_empty());
        assert!(c.ts > 0);
        assert!(!c.rel_time.is_empty());
    }

    #[test]
    fn dirty_counts_uncommitted() {
        let d = init_repo();
        assert_eq!(dirty_count(d.path()), 0);
        std::fs::write(d.path().join("b.txt"), "new").unwrap();
        assert_eq!(dirty_count(d.path()), 1);
    }

    #[test]
    fn unpushed_zero_without_upstream() {
        let d = init_repo();
        assert_eq!(unpushed_count(d.path()), 0);
    }
}
