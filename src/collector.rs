use crate::model::{LastCommit, RepoStatus};
use crate::{gh, git, next, planning, progress};
use std::path::Path;

pub fn collect(repo: &Path, gh_bin: &str, use_gh: bool) -> RepoStatus {
    let name = repo
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path = repo.display().to_string();

    if !repo.join(".git").exists() {
        return RepoStatus {
            name,
            path,
            error: Some("not a git repo".into()),
            ..Default::default()
        };
    }

    let commit = git::last_commit(repo);
    let commit_ts = commit.as_ref().map(|c| c.ts).unwrap_or(0);
    let last_commit = commit.map(|c| LastCommit {
        rel_time: c.rel_time,
        message: c.message,
        sha: c.sha,
    });

    let gh_info = if use_gh {
        gh::collect(repo, gh_bin)
    } else {
        gh::GhInfo::unavailable()
    };

    RepoStatus {
        name,
        path,
        branch: git::branch(repo),
        last_commit,
        dirty_files: git::dirty_count(repo),
        unpushed: git::unpushed_count(repo),
        open_prs: gh_info.prs,
        open_issues: gh_info.open_issues,
        gh_available: gh_info.available,
        progress_tail: progress::tail(repo),
        next_actions: next::read_open(repo),
        planning_docs: planning::detect(repo),
        commit_ts,
        error: None,
    }
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
            Command::new("git")
                .current_dir(p)
                .args(args)
                .output()
                .unwrap();
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.email", "t@t.com"]);
        run(&["config", "user.name", "t"]);
        std::fs::write(p.join("progress.md"), "## Now\nworking on X\n").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn collects_git_repo_with_gh_unavailable() {
        let d = init_repo();
        let s = collect(d.path(), "wip-no-such-gh-binary-xyz", true);
        assert_eq!(s.branch, "main");
        assert!(s.error.is_none());
        assert!(!s.gh_available);
        assert_eq!(s.last_commit.unwrap().message, "init");
        assert!(s.progress_tail.unwrap().contains("working on X"));
        assert!(s.commit_ts > 0);
    }

    #[test]
    fn non_git_dir_yields_error() {
        let d = TempDir::new().unwrap();
        let s = collect(d.path(), "wip-no-such-gh-binary-xyz", true);
        assert_eq!(s.error.as_deref(), Some("not a git repo"));
    }

    #[test]
    fn collects_next_actions_and_planning_docs() {
        let d = init_repo();
        std::fs::write(d.path().join("NEXT.md"), "- [ ] ship v2\n- [x] done\n").unwrap();
        std::fs::write(d.path().join("ROADMAP.md"), "# roadmap\n").unwrap();
        let s = collect(d.path(), "wip-no-such-gh-binary-xyz", true);
        assert_eq!(s.next_actions, vec!["ship v2".to_string()]);
        assert_eq!(s.planning_docs, vec!["ROADMAP.md".to_string()]);
    }

    #[test]
    fn use_gh_false_skips_gh() {
        let d = init_repo();
        // Even passing a real "gh" name, use_gh=false must not consult gh.
        let s = collect(d.path(), "gh", false);
        assert!(!s.gh_available);
        assert!(s.open_prs.is_empty());
        assert!(s.open_issues.is_none());
    }
}
