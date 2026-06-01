# wip — Cross-Repo Dev Status CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Rust CLI `wip` that aggregates git state, open GitHub PRs/issues, and `progress.md` tails across a curated list of repos, rendering to terminal / markdown / json so both yazelin and Claude Code consume the same source of truth.

**Architecture:** Three independent units behind a thin `main`: `config` (which repos to scan) → `collector` (per-repo git + gh + progress, parallel) → `render` (term/md/json). A shared `model::RepoStatus` is the intermediate contract. Each unit is pure-ish and unit-tested in isolation; gh failures degrade gracefully.

**Tech Stack:** Rust 2021, `clap` (CLI), `serde`/`serde_json` (json), `toml` (config), `tempfile` (dev-dep, fixture repos). Git/gh invoked via `std::process::Command`. No emoji anywhere in output (plain ASCII markers).

---

## File Structure

```
wip/
  Cargo.toml
  src/
    main.rs          # wire: parse cli -> resolve repos -> parallel collect -> sort -> render
    cli.rs           # clap arg parsing (Cli struct)
    model.rs         # RepoStatus, LastCommit, OpenPr (the shared contract)
    git.rs           # branch / last_commit / dirty_count / unpushed_count
    gh.rs            # collect open PRs + issue count, graceful degrade
    progress.rs      # find progress.md, extract last section
    config.rs        # parse repos.toml + scan_root + default path
    collector.rs     # collect one repo -> RepoStatus (orchestrates git+gh+progress)
    render.rs        # json / markdown / term renderers
  repos.example.toml # seed config to copy to ~/.config/wip/repos.toml
  README.md
```

Each `src/*.rs` has one responsibility and inline `#[cfg(test)] mod tests`. `v1` leaves `RepoStatus.next_actions` always empty (v2 fills it from `exchange`).

---

### Task 1: Scaffold project + shared model

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs` (temporary stub)
- Create: `src/model.rs`

- [ ] **Step 1: Write Cargo.toml**

```toml
[package]
name = "wip"
version = "0.1.0"
edition = "2021"
description = "Cross-repo dev status board for humans and AI agents"

[[bin]]
name = "wip"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Write the failing test in `src/model.rs`**

```rust
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct LastCommit {
    pub rel_time: String,
    pub message: String,
    pub sha: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OpenPr {
    pub number: u64,
    pub title: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RepoStatus {
    pub name: String,
    pub path: String,
    pub branch: String,
    pub last_commit: Option<LastCommit>,
    pub dirty_files: u32,
    pub unpushed: u32,
    pub open_prs: Vec<OpenPr>,
    pub open_issues: Option<u32>,
    pub gh_available: bool,
    pub progress_tail: Option<String>,
    pub next_actions: Vec<String>, // v2: filled from exchange; always empty in v1
    pub commit_ts: i64,            // committer unix time, for sorting
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_status_default_is_empty() {
        let r = RepoStatus::default();
        assert_eq!(r.name, "");
        assert_eq!(r.dirty_files, 0);
        assert!(r.last_commit.is_none());
        assert!(r.next_actions.is_empty());
    }
}
```

- [ ] **Step 3: Write temporary `src/main.rs` so the crate compiles**

```rust
mod model;

fn main() {
    println!("wip stub");
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --lib model 2>&1 | tail -20`
Expected: `test model::tests::repo_status_default_is_empty ... ok` (cargo downloads deps on first run)

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/main.rs src/model.rs
git commit -m "feat: scaffold wip crate with RepoStatus model"
```

---

### Task 2: git helpers

**Files:**
- Create: `src/git.rs`
- Modify: `src/main.rs` (add `mod git;`)

- [ ] **Step 1: Write the failing test in `src/git.rs`**

```rust
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
```

- [ ] **Step 2: Add `mod git;` to `src/main.rs`**

Edit `src/main.rs` top to read:

```rust
mod model;
mod git;

fn main() {
    println!("wip stub");
}
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test --lib git 2>&1 | tail -20`
Expected: 4 tests pass (`branch_reports_main`, `last_commit_parsed`, `dirty_counts_uncommitted`, `unpushed_zero_without_upstream`)

- [ ] **Step 4: Commit**

```bash
git add src/git.rs src/main.rs
git commit -m "feat: git status helpers (branch, last commit, dirty, unpushed)"
```

---

### Task 3: gh helpers with graceful degrade

**Files:**
- Create: `src/gh.rs`
- Modify: `src/main.rs` (add `mod gh;`)

- [ ] **Step 1: Write the failing test in `src/gh.rs`**

```rust
use crate::model::OpenPr;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Deserialize)]
struct GhPr {
    number: u64,
    title: String,
}

#[derive(Deserialize)]
struct GhIssue {
    #[allow(dead_code)]
    number: u64,
}

pub struct GhInfo {
    pub available: bool,
    pub prs: Vec<OpenPr>,
    pub open_issues: Option<u32>,
}

fn gh_stdout(repo: &Path, gh_bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(gh_bin)
        .current_dir(repo)
        .args(args)
        .output()
        .ok()?; // spawn error (gh not installed) -> None
    if !out.status.success() {
        return None; // not a gh repo / not authed -> None
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn collect(repo: &Path, gh_bin: &str) -> GhInfo {
    let prs_raw = gh_stdout(repo, gh_bin, &["pr", "list", "--json", "number,title"]);
    let issues_raw = gh_stdout(repo, gh_bin, &["issue", "list", "--json", "number"]);

    match (prs_raw, issues_raw) {
        (Some(pj), Some(ij)) => {
            let prs: Vec<GhPr> = serde_json::from_str(&pj).unwrap_or_default();
            let issues: Vec<GhIssue> = serde_json::from_str(&ij).unwrap_or_default();
            GhInfo {
                available: true,
                prs: prs
                    .into_iter()
                    .map(|p| OpenPr {
                        number: p.number,
                        title: p.title,
                    })
                    .collect(),
                open_issues: Some(issues.len() as u32),
            }
        }
        _ => GhInfo {
            available: false,
            prs: vec![],
            open_issues: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn missing_gh_binary_degrades() {
        let info = collect(Path::new("/tmp"), "wip-no-such-gh-binary-xyz");
        assert!(!info.available);
        assert!(info.prs.is_empty());
        assert!(info.open_issues.is_none());
    }
}
```

- [ ] **Step 2: Add `mod gh;` to `src/main.rs`**

Edit the `mod` block in `src/main.rs`:

```rust
mod model;
mod git;
mod gh;
```

- [ ] **Step 3: Run the test to verify it passes**

Run: `cargo test --lib gh 2>&1 | tail -20`
Expected: `test gh::tests::missing_gh_binary_degrades ... ok`

- [ ] **Step 4: Commit**

```bash
git add src/gh.rs src/main.rs
git commit -m "feat: gh PR/issue collector with graceful degrade"
```

---

### Task 4: progress.md tail extraction

**Files:**
- Create: `src/progress.rs`
- Modify: `src/main.rs` (add `mod progress;`)

- [ ] **Step 1: Write the failing test in `src/progress.rs`**

```rust
use std::fs;
use std::path::Path;

const CANDIDATES: &[&str] = &["progress.md", "PROGRESS.md", "docs/progress.md"];
const CAP: usize = 400;

pub fn tail(repo: &Path) -> Option<String> {
    for c in CANDIDATES {
        let p = repo.join(c);
        if p.is_file() {
            let content = fs::read_to_string(&p).ok()?;
            return Some(last_section(&content));
        }
    }
    None
}

/// Return the last `## ` section of a markdown doc, trimmed and capped.
/// If there are no `## ` headings, return the whole (trimmed, capped) text.
fn last_section(content: &str) -> String {
    let mut sections: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in content.lines() {
        if line.starts_with("## ") && !cur.trim().is_empty() {
            sections.push(std::mem::take(&mut cur));
        }
        cur.push_str(line);
        cur.push('\n');
    }
    if !cur.trim().is_empty() {
        sections.push(cur);
    }
    let last = sections.last().map(|s| s.trim()).unwrap_or("");
    if last.chars().count() > CAP {
        let truncated: String = last.chars().take(CAP).collect();
        format!("{truncated}...")
    } else {
        last.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn last_section_returns_final_heading_block() {
        let md = "## Old\nold stuff\n\n## New\nnew stuff here\n";
        let out = last_section(md);
        assert!(out.starts_with("## New"));
        assert!(out.contains("new stuff here"));
        assert!(!out.contains("old stuff"));
    }

    #[test]
    fn no_headings_returns_whole_text() {
        let md = "just a note\nsecond line\n";
        let out = last_section(md);
        assert!(out.contains("just a note"));
        assert!(out.contains("second line"));
    }

    #[test]
    fn tail_reads_progress_file() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("progress.md"), "## Done\nshipped it\n").unwrap();
        let out = tail(d.path()).expect("tail");
        assert!(out.contains("shipped it"));
    }

    #[test]
    fn tail_none_when_absent() {
        let d = TempDir::new().unwrap();
        assert!(tail(d.path()).is_none());
    }
}
```

- [ ] **Step 2: Add `mod progress;` to `src/main.rs`**

```rust
mod model;
mod git;
mod gh;
mod progress;
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test --lib progress 2>&1 | tail -20`
Expected: 4 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/progress.rs src/main.rs
git commit -m "feat: progress.md last-section extraction"
```

---

### Task 5: config resolution (repos.toml + --root scan)

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs` (add `mod config;`)

- [ ] **Step 1: Write the failing test in `src/config.rs`**

```rust
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
```

- [ ] **Step 2: Add `mod config;` to `src/main.rs`**

```rust
mod model;
mod git;
mod gh;
mod progress;
mod config;
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test --lib config 2>&1 | tail -20`
Expected: 3 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/config.rs src/main.rs
git commit -m "feat: config resolution (repos.toml + --root scan)"
```

---

### Task 6: collector (one repo -> RepoStatus)

**Files:**
- Create: `src/collector.rs`
- Modify: `src/main.rs` (add `mod collector;`)

- [ ] **Step 1: Write the failing test in `src/collector.rs`**

```rust
use crate::model::{LastCommit, RepoStatus};
use crate::{gh, git, progress};
use std::path::Path;

pub fn collect(repo: &Path, gh_bin: &str) -> RepoStatus {
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

    let gh_info = gh::collect(repo, gh_bin);

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
        next_actions: vec![], // v2
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
        let s = collect(d.path(), "wip-no-such-gh-binary-xyz");
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
        let s = collect(d.path(), "wip-no-such-gh-binary-xyz");
        assert_eq!(s.error.as_deref(), Some("not a git repo"));
    }
}
```

- [ ] **Step 2: Add `mod collector;` to `src/main.rs`**

```rust
mod model;
mod git;
mod gh;
mod progress;
mod config;
mod collector;
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test --lib collector 2>&1 | tail -20`
Expected: 2 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/collector.rs src/main.rs
git commit -m "feat: collector assembles RepoStatus from git+gh+progress"
```

---

### Task 7: render — json

**Files:**
- Create: `src/render.rs`
- Modify: `src/main.rs` (add `mod render;`)

- [ ] **Step 1: Write the failing test in `src/render.rs`**

```rust
use crate::model::RepoStatus;

pub fn json(statuses: &[RepoStatus]) -> String {
    serde_json::to_string_pretty(statuses).unwrap_or_else(|_| "[]".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LastCommit, RepoStatus};

    fn sample() -> RepoStatus {
        RepoStatus {
            name: "demo".into(),
            path: "/x/demo".into(),
            branch: "main".into(),
            last_commit: Some(LastCommit {
                rel_time: "2 hours ago".into(),
                message: "did thing".into(),
                sha: "abc123".into(),
            }),
            dirty_files: 2,
            commit_ts: 1000,
            ..Default::default()
        }
    }

    #[test]
    fn json_roundtrips_fields() {
        let out = json(&[sample()]);
        assert!(out.contains("\"name\": \"demo\""));
        assert!(out.contains("\"branch\": \"main\""));
        assert!(out.contains("did thing"));
        // valid json array
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v.is_array());
    }
}
```

- [ ] **Step 2: Add `mod render;` to `src/main.rs`**

```rust
mod model;
mod git;
mod gh;
mod progress;
mod config;
mod collector;
mod render;
```

- [ ] **Step 3: Run the test to verify it passes**

Run: `cargo test --lib render::tests::json 2>&1 | tail -20`
Expected: `test render::tests::json_roundtrips_fields ... ok`

- [ ] **Step 4: Commit**

```bash
git add src/render.rs src/main.rs
git commit -m "feat: json renderer"
```

---

### Task 8: render — markdown

**Files:**
- Modify: `src/render.rs`

- [ ] **Step 1: Write the failing test (append inside `src/render.rs`)**

Add this function above the `#[cfg(test)]` module:

```rust
/// Markdown digest for AI agents to ingest. ASCII only, no emoji.
pub fn markdown(statuses: &[RepoStatus]) -> String {
    let mut s = String::from("# wip - dev status\n\n");
    for r in statuses {
        s.push_str(&format!("## {} ({})\n", r.name, r.branch));
        if let Some(e) = &r.error {
            s.push_str(&format!("- error: {e}\n\n"));
            continue;
        }
        if let Some(c) = &r.last_commit {
            s.push_str(&format!(
                "- last: {} - {} ({})\n",
                c.rel_time, c.message, c.sha
            ));
        }
        let mut flags = Vec::new();
        if r.dirty_files > 0 {
            flags.push(format!("{} dirty", r.dirty_files));
        }
        if r.unpushed > 0 {
            flags.push(format!("{} unpushed", r.unpushed));
        }
        if !flags.is_empty() {
            s.push_str(&format!("- local: {}\n", flags.join(", ")));
        }
        if r.gh_available {
            if r.open_prs.is_empty() {
                s.push_str("- PRs: none\n");
            } else {
                for pr in &r.open_prs {
                    s.push_str(&format!("- PR #{}: {}\n", pr.number, pr.title));
                }
            }
            if let Some(i) = r.open_issues {
                s.push_str(&format!("- open issues: {i}\n"));
            }
        } else {
            s.push_str("- PRs: - (gh unavailable)\n");
        }
        if let Some(p) = &r.progress_tail {
            let first = p.lines().next().unwrap_or("");
            s.push_str(&format!("- progress: {first}\n"));
        }
        s.push('\n');
    }
    s
}
```

Add this test inside `mod tests`:

```rust
    #[test]
    fn markdown_includes_repo_and_commit() {
        let out = markdown(&[sample()]);
        assert!(out.contains("## demo (main)"));
        assert!(out.contains("did thing"));
        assert!(out.contains("2 dirty"));
        assert!(out.contains("gh unavailable")); // sample has gh_available=false
        // no emoji: assert the warn/check symbols are absent
        assert!(!out.contains('\u{26A0}')); // no WARNING SIGN
        assert!(!out.contains('\u{2713}')); // no CHECK MARK
    }
```

- [ ] **Step 2: Run the test to verify it passes**

Run: `cargo test --lib render::tests::markdown 2>&1 | tail -20`
Expected: `test render::tests::markdown_includes_repo_and_commit ... ok`

- [ ] **Step 3: Commit**

```bash
git add src/render.rs
git commit -m "feat: markdown renderer (ascii, no emoji)"
```

---

### Task 9: render — terminal

**Files:**
- Modify: `src/render.rs`

- [ ] **Step 1: Write the failing test (append inside `src/render.rs`)**

Add this function above the `#[cfg(test)]` module:

```rust
/// Human-facing terminal output. ASCII only, no emoji. Indented blocks.
pub fn term(statuses: &[RepoStatus]) -> String {
    let mut s = format!("wip - {} repos (recent first)\n\n", statuses.len());
    for r in statuses {
        if let Some(e) = &r.error {
            s.push_str(&format!("{}  [error: {}]\n\n", r.name, e));
            continue;
        }
        s.push_str(&format!("{}  {}\n", r.name, r.branch));
        if let Some(c) = &r.last_commit {
            let mut line = format!("  last {}  \"{}\"", c.rel_time, c.message);
            let mut flags = Vec::new();
            if r.dirty_files > 0 {
                flags.push(format!("{} dirty", r.dirty_files));
            }
            if r.unpushed > 0 {
                flags.push(format!("{} unpushed", r.unpushed));
            }
            if !flags.is_empty() {
                line.push_str(&format!("  [{}]", flags.join(", ")));
            }
            s.push_str(&line);
            s.push('\n');
        }
        let pr_part = if !r.gh_available {
            "PR: -".to_string()
        } else if r.open_prs.is_empty() {
            "PR: none".to_string()
        } else {
            let list: Vec<String> = r
                .open_prs
                .iter()
                .map(|p| format!("#{}", p.number))
                .collect();
            format!("PR: {}", list.join(" "))
        };
        let issue_part = match r.open_issues {
            Some(i) => format!("   issues: {i}"),
            None => String::new(),
        };
        let progress_part = match &r.progress_tail {
            Some(p) => format!("   progress: {}", p.lines().next().unwrap_or("")),
            None => String::new(),
        };
        s.push_str(&format!("  {pr_part}{issue_part}{progress_part}\n\n"));
    }
    s
}
```

Add this test inside `mod tests`:

```rust
    #[test]
    fn term_renders_name_and_status() {
        let out = term(&[sample()]);
        assert!(out.contains("demo  main"));
        assert!(out.contains("\"did thing\""));
        assert!(out.contains("2 dirty"));
        assert!(out.contains("PR: -")); // gh unavailable in sample
        assert!(!out.contains('\u{26A0}')); // no emoji
    }
```

- [ ] **Step 2: Run the test to verify it passes**

Run: `cargo test --lib render::tests::term 2>&1 | tail -20`
Expected: `test render::tests::term_renders_name_and_status ... ok`

- [ ] **Step 3: Run full library suite to confirm nothing regressed**

Run: `cargo test --lib 2>&1 | tail -20`
Expected: all tests pass (model 1, git 4, gh 1, progress 4, config 3, collector 2, render 3)

- [ ] **Step 4: Commit**

```bash
git add src/render.rs
git commit -m "feat: terminal renderer (ascii, no emoji)"
```

---

### Task 10: CLI + main wiring (parallel collect, sort)

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs` (replace stub with real wiring)

- [ ] **Step 1: Write `src/cli.rs`**

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "wip", about = "Cross-repo dev status for humans and AI agents")]
pub struct Cli {
    /// Output markdown (for AI agents to ingest)
    #[arg(long)]
    pub md: bool,

    /// Output json
    #[arg(long)]
    pub json: bool,

    /// Scan a directory's immediate subdirs for git repos (ignores config)
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Config file path (default: ~/.config/wip/repos.toml)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// gh binary to use
    #[arg(long, default_value = "gh")]
    pub gh_bin: String,
}
```

- [ ] **Step 2: Replace `src/main.rs` entirely with real wiring**

```rust
mod cli;
mod collector;
mod config;
mod gh;
mod git;
mod model;
mod progress;
mod render;

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;

fn resolve_repos(args: &cli::Cli) -> Result<Vec<PathBuf>, String> {
    if let Some(root) = &args.root {
        let repos = config::scan_root(root);
        if repos.is_empty() {
            return Err(format!("no git repos found under {}", root.display()));
        }
        return Ok(repos);
    }
    let cfg_path = args
        .config
        .clone()
        .unwrap_or_else(config::default_config_path);
    config::load_from_file(&cfg_path).map_err(|e| {
        format!("{e}\nCreate it, e.g.: cp repos.example.toml {}", cfg_path.display())
    })
}

fn run() -> Result<String, String> {
    let args = cli::Cli::parse();
    let repos = resolve_repos(&args)?;

    // Collect repos in parallel; git+gh subprocesses dominate wall time.
    let gh_bin = args.gh_bin.clone();
    let mut statuses: Vec<model::RepoStatus> = thread::scope(|s| {
        let handles: Vec<_> = repos
            .iter()
            .map(|r| s.spawn(|| collector::collect(r, &gh_bin)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    // Most recently committed first; error/empty repos (commit_ts 0) sink to bottom.
    statuses.sort_by(|a, b| b.commit_ts.cmp(&a.commit_ts));

    let out = if args.json {
        render::json(&statuses)
    } else if args.md {
        render::markdown(&statuses)
    } else {
        render::term(&statuses)
    };
    Ok(out)
}

fn main() -> ExitCode {
    match run() {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("wip: {e}");
            ExitCode::FAILURE
        }
    }
}
```

- [ ] **Step 3: Build and run against this repo's own dir as a smoke test**

Run:
```bash
cargo build 2>&1 | tail -5
./target/debug/wip --root .. 2>&1 | head -30
```
Expected: builds clean; prints a `wip - N repos` block listing sibling repos under the parent dir with branches and last commits. (gh lines show `PR: -` if gh is absent/unauthed — that is correct degraded behavior.)

- [ ] **Step 4: Smoke-test json and md modes**

Run:
```bash
./target/debug/wip --root .. --json | head -5
./target/debug/wip --root .. --md | head -15
```
Expected: `--json` prints a valid JSON array; `--md` prints `# wip - dev status` followed by `## <repo> (<branch>)` blocks.

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: wire cli + parallel collect + recent-first sort"
```

---

### Task 11: seed config, README, install

**Files:**
- Create: `repos.example.toml`
- Create: `README.md`

- [ ] **Step 1: Write `repos.example.toml`**

```toml
# Copy to ~/.config/wip/repos.toml and edit.
# Curated list of repos to track — keep it to what's actively in flight.
repos = [
  "~/mori-universe/mori-meeting-recorder",
  "~/mori-universe/mori-desktop",
  "~/mori-universe/mori-ear",
  "~/mori-universe/annuli",
  "~/agentos",
  "~/agentos-notebook",
  "~/smriti",
  "~/exchange",
  "~/yazelin.github.io",
]
```

- [ ] **Step 2: Write `README.md`**

````markdown
# wip — cross-repo dev status

One command to see where every project stands: current branch, last commit,
dirty/unpushed state, open PRs/issues, and the tail of each repo's `progress.md`.
Built so **both you and Claude Code read the same source of truth** — humans get a
terminal table, agents run `wip --md` and ingest the markdown.

## Install

```bash
cargo install --path .          # installs `wip` into ~/.cargo/bin
mkdir -p ~/.config/wip
cp repos.example.toml ~/.config/wip/repos.toml   # then edit the list
```

## Usage

```bash
wip                 # terminal table, most recently committed repo first
wip --md            # markdown digest (for Claude Code / SessionStart hook)
wip --json          # structured output
wip --root ~/some/dir   # ad-hoc: scan immediate subdirs for git repos, ignore config
```

`gh` is optional — if it's missing or you're not authed, PR/issue columns show `-`
and git status still works.

## Roadmap

- **v1 (done):** read-only status across a curated repo list.
- **v2:** active management — `wip next <repo> "<next step>"` stores next-actions in
  the [`exchange`](../exchange) mailbox (`kind=next`), and `wip` renders a "next" line.
  Shared store means you and Claude Code see each other's notes.

## Output has no emoji

By preference, all output is plain ASCII (`[3 dirty]`, `PR: none`) — no emoji or
symbol glyphs.
````

- [ ] **Step 3: Verify the example config loads**

Run:
```bash
cargo run -- --config repos.example.toml 2>&1 | head -20
```
Expected: prints status for whichever listed repos exist on this machine (missing paths show `[error: not a git repo]` and sink to the bottom — acceptable). No crash.

- [ ] **Step 4: Commit**

```bash
git add repos.example.toml README.md
git commit -m "docs: seed config + README + install instructions"
```

---

## Self-Review

**Spec coverage:**
- CLI not TUI, term/md/json output → Tasks 7-10 (renderers + cli flags). ✓
- Rust single binary → Cargo `[[bin]]` Task 1, `cargo install` README Task 11. ✓
- config: curated repos.toml + `--root` scan → Task 5 + Task 10 `resolve_repos`. ✓
- collector: git (branch/last commit/dirty/unpushed) → Task 2; gh PR/issue graceful degrade → Task 3; progress.md tail → Task 4; assembled → Task 6. ✓
- renderer term/md/json, recent-first sort → Tasks 7-9 + sort in Task 10. ✓
- RepoStatus intermediate structure → Task 1 model.rs. ✓
- Error handling: per-repo error non-fatal (collector returns error field, others continue via parallel join) → Task 6 + Task 10; gh absent degrades → Task 3. ✓
- v2 boundary: `next_actions` field present but empty; exchange wiring deferred → Task 1 model + README roadmap. ✓ (v2 intentionally not built)
- No-emoji preference applied → Tasks 8-9 assert absence of glyphs. ✓

**Placeholder scan:** No TBD/TODO; every code step shows full code; every run step shows command + expected output. ✓

**Type consistency:** `RepoStatus`/`LastCommit`/`OpenPr` field names defined in Task 1 are used identically in Tasks 6-10 (`commit_ts`, `gh_available`, `progress_tail`, `next_actions`, `last_commit`, `dirty_files`, `unpushed`, `open_prs`, `open_issues`). `git::last_commit`/`branch`/`dirty_count`/`unpushed_count` (Task 2) match calls in collector (Task 6). `gh::collect` returns `GhInfo{available,prs,open_issues}` (Task 3) consumed identically in Task 6. `render::{json,markdown,term}` (Tasks 7-9) match calls in main (Task 10). `config::{load_from_file,scan_root,default_config_path}` (Task 5) match `resolve_repos` (Task 10). ✓

**Open spec items** (deferred by design, not gaps): progress.md section-cut uses `## ` headings (locked in Task 4); sort uses committer timestamp `commit_ts` (locked in Task 10). Both were flagged "decide at implementation" in the spec and are now decided.
