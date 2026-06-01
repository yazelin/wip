# wip v2 — next-actions via NEXT.md + planning-file pointers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-repo next-action management to `wip` (read/add/complete items in each repo's root `NEXT.md`) plus a deterministic "detected planning files" pointer line (roadmap/TODO/...), so the board both carries explicit next-actions and tells Claude where to dig deeper. No exchange, no new dependencies.

**Architecture:** `src/next.rs` parses/mutates `NEXT.md` (open `- [ ]` / done `- [x]`). `src/planning.rs` detects common planning filenames in the repo root (names only, no content parsing). The collector fills two `RepoStatus` fields — existing `next_actions` and new `planning_docs`. Renderers show open next-actions numbered and a `see:` pointer line. `main` gains `next`/`done` subcommands resolving a repo name/path and writing `NEXT.md`.

**Tech Stack:** Rust 2021, existing deps only (clap, serde, serde_json, toml, tempfile). No new crates.

**IMPORTANT:** bin-only crate — use `cargo test <filter>` / `cargo build` / `cargo run`, NOT `cargo test --lib`. Build on branch `feat/v2-next-actions` (create it off `main` first).

---

### Task 1: NEXT.md parse + mutate module

**Files:**
- Create: `src/next.rs`
- Modify: `src/main.rs` (add `mod next;`)

- [ ] **Step 1: Write `src/next.rs` with this EXACT content**

```rust
use std::fs;
use std::path::{Path, PathBuf};

fn next_path(repo: &Path) -> PathBuf {
    repo.join("NEXT.md")
}

/// An open item is a line that, after trimming leading whitespace, starts with
/// "- [ ] ". Returns the text after that prefix.
fn parse_open(line: &str) -> Option<String> {
    line.trim_start().strip_prefix("- [ ] ").map(|s| s.to_string())
}

/// Open next-action texts from `<repo>/NEXT.md`, in file order.
/// Missing/unreadable file -> empty (not an error).
pub fn read_open(repo: &Path) -> Vec<String> {
    let content = match fs::read_to_string(next_path(repo)) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    content.lines().filter_map(parse_open).collect()
}

/// Append "- [ ] <text>" to `<repo>/NEXT.md`, creating the file (with a header)
/// if absent.
pub fn add(repo: &Path, text: &str) -> std::io::Result<()> {
    let path = next_path(repo);
    let mut content = fs::read_to_string(&path).unwrap_or_default();
    if content.is_empty() {
        content.push_str("# Next\n\n");
    } else if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("- [ ] {text}\n"));
    fs::write(&path, content)
}

/// Flip the n-th OPEN item (1-based) to "- [x] ", preserving leading whitespace
/// and text. Returns the completed item's text. Errors if n is out of range.
pub fn mark_done(repo: &Path, n: usize) -> Result<String, String> {
    let path = next_path(repo);
    let content =
        fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut open_seen = 0;
    let mut done_text = None;
    for line in lines.iter_mut() {
        if let Some(text) = parse_open(line) {
            open_seen += 1;
            if open_seen == n {
                let ws_len = line.len() - line.trim_start().len();
                let ws = line[..ws_len].to_string();
                *line = format!("{ws}- [x] {text}");
                done_text = Some(text);
                break;
            }
        }
    }
    match done_text {
        Some(t) => {
            let mut out = lines.join("\n");
            if content.ends_with('\n') {
                out.push('\n');
            }
            fs::write(&path, out).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            Ok(t)
        }
        None => Err(format!("no open item #{n} (only {open_seen} open)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_next(d: &Path, body: &str) {
        std::fs::write(d.join("NEXT.md"), body).unwrap();
    }

    #[test]
    fn read_open_returns_only_open_items_in_order() {
        let d = TempDir::new().unwrap();
        write_next(
            d.path(),
            "# Next\n\n- [ ] first\n- [x] done one\n- [ ] second\nsome prose\n",
        );
        assert_eq!(read_open(d.path()), vec!["first".to_string(), "second".to_string()]);
    }

    #[test]
    fn read_open_empty_when_no_file() {
        let d = TempDir::new().unwrap();
        assert!(read_open(d.path()).is_empty());
    }

    #[test]
    fn add_creates_file_with_header() {
        let d = TempDir::new().unwrap();
        add(d.path(), "do thing").unwrap();
        let c = std::fs::read_to_string(d.path().join("NEXT.md")).unwrap();
        assert!(c.starts_with("# Next\n"));
        assert!(c.contains("- [ ] do thing\n"));
    }

    #[test]
    fn add_appends_to_existing() {
        let d = TempDir::new().unwrap();
        write_next(d.path(), "# Next\n\n- [ ] one\n");
        add(d.path(), "two").unwrap();
        assert_eq!(read_open(d.path()), vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn mark_done_flips_nth_open() {
        let d = TempDir::new().unwrap();
        write_next(d.path(), "- [ ] a\n- [ ] b\n- [ ] c\n");
        let t = mark_done(d.path(), 2).unwrap();
        assert_eq!(t, "b");
        assert_eq!(read_open(d.path()), vec!["a".to_string(), "c".to_string()]);
        let c = std::fs::read_to_string(d.path().join("NEXT.md")).unwrap();
        assert!(c.contains("- [x] b"));
    }

    #[test]
    fn mark_done_out_of_range_errors() {
        let d = TempDir::new().unwrap();
        write_next(d.path(), "- [ ] only\n");
        let e = mark_done(d.path(), 5).unwrap_err();
        assert!(e.contains("no open item"));
    }
}
```

- [ ] **Step 2: Add `mod next;` to `src/main.rs`**

Insert `mod next;` so the module block reads (alphabetical, `next` between `model` and `progress`):

```rust
mod cli;
mod collector;
mod config;
mod gh;
mod git;
mod model;
mod next;
mod progress;
mod render;
```

(`add`/`mark_done` unused until Task 5, `read_open` until Task 3 — expected warnings, do NOT add `#[allow(dead_code)]`.)

- [ ] **Step 3: Run the tests**

Run: `cargo test next 2>&1 | tail -20`
Expected: 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/next.rs src/main.rs
git commit -m "feat: NEXT.md parse + mutate module (read_open/add/mark_done)"
```

---

### Task 2: planning-file detection + model field

**Files:**
- Create: `src/planning.rs`
- Modify: `src/model.rs` (add `planning_docs` field)
- Modify: `src/main.rs` (add `mod planning;`)

- [ ] **Step 1: Write `src/planning.rs` with this EXACT content**

```rust
use std::path::Path;

/// Common planning-file names (lowercase) we surface as pointers. Content is
/// never parsed - we only report which exist.
const PLANNING_NAMES: &[&str] = &["roadmap.md", "todo.md", "todo", "plan.md", "backlog.md"];

/// Names of planning files present in the repo root (case-insensitive match),
/// sorted for deterministic output. Empty if none / unreadable.
pub fn detect(repo: &Path) -> Vec<String> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(repo) {
        for e in entries.flatten() {
            if !e.path().is_file() {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if PLANNING_NAMES.contains(&name.to_lowercase().as_str()) {
                found.push(name);
            }
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detects_planning_files_case_insensitive() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("ROADMAP.md"), "x").unwrap();
        std::fs::write(d.path().join("TODO.md"), "x").unwrap();
        std::fs::write(d.path().join("README.md"), "x").unwrap(); // not a planning file
        assert_eq!(
            detect(d.path()),
            vec!["ROADMAP.md".to_string(), "TODO.md".to_string()]
        );
    }

    #[test]
    fn empty_when_none() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("README.md"), "x").unwrap();
        assert!(detect(d.path()).is_empty());
    }
}
```

- [ ] **Step 2: Add the `planning_docs` field to `src/model.rs`**

In `src/model.rs`, in the `RepoStatus` struct, find:

```rust
    pub next_actions: Vec<String>, // v2: filled from exchange; always empty in v1
    pub commit_ts: i64,            // committer unix time, for sorting
```
Replace those two lines with:
```rust
    pub next_actions: Vec<String>,  // v2: open items from <repo>/NEXT.md
    pub planning_docs: Vec<String>, // v2: detected planning files (roadmap/TODO/...), pointers only
    pub commit_ts: i64,             // committer unix time, for sorting
```

(The field is `Default` (empty Vec) and `Serialize` via the struct's existing derives — no other change to model.rs.)

- [ ] **Step 3: Add `mod planning;` to `src/main.rs`**

Insert `mod planning;` so the block reads (`planning` between `next` and `progress`):

```rust
mod cli;
mod collector;
mod config;
mod gh;
mod git;
mod model;
mod next;
mod planning;
mod progress;
mod render;
```

- [ ] **Step 4: Run the tests**

Run: `cargo test planning 2>&1 | tail -15`
Expected: 2 tests pass (`detects_planning_files_case_insensitive`, `empty_when_none`). The crate compiles with the new model field (the `model::tests::repo_status_default_is_empty` test still passes — `planning_docs` defaults to empty).

- [ ] **Step 5: Commit**

```bash
git add src/planning.rs src/model.rs src/main.rs
git commit -m "feat: planning-file detection + RepoStatus.planning_docs field"
```

---

### Task 3: collector fills next_actions + planning_docs

**Files:**
- Modify: `src/collector.rs`

- [ ] **Step 1: Write the failing test (add inside the existing `mod tests` in `src/collector.rs`)**

Add this test alongside the existing collector tests (reuses the existing `init_repo()` helper):

```rust
    #[test]
    fn collects_next_actions_and_planning_docs() {
        let d = init_repo();
        std::fs::write(d.path().join("NEXT.md"), "- [ ] ship v2\n- [x] done\n").unwrap();
        std::fs::write(d.path().join("ROADMAP.md"), "# roadmap\n").unwrap();
        let s = collect(d.path(), "wip-no-such-gh-binary-xyz");
        assert_eq!(s.next_actions, vec!["ship v2".to_string()]);
        assert_eq!(s.planning_docs, vec!["ROADMAP.md".to_string()]);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test collector::tests::collects_next_actions_and_planning_docs 2>&1 | tail -15`
Expected: FAIL — `next_actions` is currently `vec![]` and `planning_docs` defaults empty.

- [ ] **Step 3: Wire the collector**

In `src/collector.rs`, change the imports line:

```rust
use crate::{gh, git, progress};
```
to:
```rust
use crate::{gh, git, next, planning, progress};
```

Then in the returned `RepoStatus { ... }` literal, change:

```rust
        next_actions: vec![], // v2
```
to:
```rust
        next_actions: next::read_open(repo),
        planning_docs: planning::detect(repo),
```

- [ ] **Step 4: Run the tests**

Run: `cargo test collector 2>&1 | tail -15`
Expected: all collector tests pass (the non-git early-return path uses `..Default::default()`, so `planning_docs` is empty there — still fine).

- [ ] **Step 5: Commit**

```bash
git add src/collector.rs
git commit -m "feat: collector fills next_actions + planning_docs"
```

---

### Task 4: render next-actions + planning pointers (term + markdown)

**Files:**
- Modify: `src/render.rs`

- [ ] **Step 1: Write the failing tests (add inside the existing `mod tests` in `src/render.rs`)**

Add this helper and six tests alongside the existing render tests (reusing the existing `sample()` helper):

```rust
    fn sample_with_extras() -> RepoStatus {
        let mut r = sample();
        r.next_actions = vec!["do A".into(), "do B".into()];
        r.planning_docs = vec!["ROADMAP.md".into()];
        r
    }

    #[test]
    fn term_renders_next_actions_numbered() {
        let out = term(&[sample_with_extras()]);
        assert!(out.contains("next: 1. do A   2. do B"));
    }

    #[test]
    fn term_renders_see_line() {
        let out = term(&[sample_with_extras()]);
        assert!(out.contains("see: ROADMAP.md"));
    }

    #[test]
    fn term_omits_next_and_see_when_empty() {
        let out = term(&[sample()]);
        assert!(!out.contains("next:"));
        assert!(!out.contains("see:"));
    }

    #[test]
    fn markdown_renders_next_actions_numbered() {
        let out = markdown(&[sample_with_extras()]);
        assert!(out.contains("- next:"));
        assert!(out.contains("1. do A"));
        assert!(out.contains("2. do B"));
    }

    #[test]
    fn markdown_renders_see_line() {
        let out = markdown(&[sample_with_extras()]);
        assert!(out.contains("- see: ROADMAP.md"));
    }

    #[test]
    fn markdown_omits_see_when_empty() {
        let out = markdown(&[sample()]);
        assert!(!out.contains("- see:"));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test render 2>&1 | tail -15`
Expected: the new tests FAIL (renderers don't emit next/see lines yet); existing render tests still pass.

- [ ] **Step 3: Update the `term` renderer**

In `src/render.rs`, in `term`, find:

```rust
        s.push_str(&format!("  {pr_part}{issue_part}{progress_part}\n\n"));
```
Replace it with:
```rust
        s.push_str(&format!("  {pr_part}{issue_part}{progress_part}\n"));
        if !r.next_actions.is_empty() {
            let items: Vec<String> = r
                .next_actions
                .iter()
                .enumerate()
                .map(|(i, t)| format!("{}. {}", i + 1, t))
                .collect();
            s.push_str(&format!("  next: {}\n", items.join("   ")));
        }
        if !r.planning_docs.is_empty() {
            s.push_str(&format!("  see: {}\n", r.planning_docs.join(", ")));
        }
        s.push('\n');
```

- [ ] **Step 4: Update the `markdown` renderer**

In `src/render.rs`, in `markdown`, find:

```rust
        if let Some(p) = &r.progress_tail {
            let first = p.lines().next().unwrap_or("");
            s.push_str(&format!("- progress: {first}\n"));
        }
        s.push('\n');
```
Replace it with:
```rust
        if let Some(p) = &r.progress_tail {
            let first = p.lines().next().unwrap_or("");
            s.push_str(&format!("- progress: {first}\n"));
        }
        if !r.next_actions.is_empty() {
            s.push_str("- next:\n");
            for (i, t) in r.next_actions.iter().enumerate() {
                s.push_str(&format!("  {}. {}\n", i + 1, t));
            }
        }
        if !r.planning_docs.is_empty() {
            s.push_str(&format!("- see: {}\n", r.planning_docs.join(", ")));
        }
        s.push('\n');
```

- [ ] **Step 5: Run the full suite**

Run: `cargo test 2>&1 | tail -8`
Expected: all tests pass (v1 18 + next 6 + planning 2 + collector 1 + render 6 = 33).

- [ ] **Step 6: Commit**

```bash
git add src/render.rs
git commit -m "feat: render next-actions (numbered) + planning-file see line"
```

---

### Task 5: `next` / `done` subcommands + main routing

**Files:**
- Modify: `src/cli.rs` (add subcommand enum)
- Modify: `src/main.rs` (replace with subcommand routing + `resolve_repo`)

- [ ] **Step 1: Replace `src/cli.rs` entirely with this content**

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "wip", about = "Cross-repo dev status for humans and AI agents")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

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

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Add a next-action to <repo>/NEXT.md
    Next {
        /// Repo name (basename in config) or path to a repo
        repo: String,
        /// The next-action text
        text: String,
    },
    /// Mark the n-th open next-action done in <repo>/NEXT.md
    Done {
        /// Repo name (basename in config) or path to a repo
        repo: String,
        /// 1-based index among OPEN items, as numbered by the board
        n: usize,
    },
}
```

- [ ] **Step 2: Replace `src/main.rs` entirely with this content**

```rust
mod cli;
mod collector;
mod config;
mod gh;
mod git;
mod model;
mod next;
mod planning;
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

/// Resolve a `<repo>` argument to a path: an existing directory is used directly,
/// otherwise match by basename against the configured repo list.
fn resolve_repo(name: &str, args: &cli::Cli) -> Result<PathBuf, String> {
    let direct = PathBuf::from(name);
    if direct.is_dir() {
        return Ok(direct);
    }
    let cfg_path = args
        .config
        .clone()
        .unwrap_or_else(config::default_config_path);
    let repos = config::load_from_file(&cfg_path)?;
    repos
        .into_iter()
        .find(|r| r.file_name().and_then(|s| s.to_str()) == Some(name))
        .ok_or_else(|| format!("repo '{name}' not found in config {}", cfg_path.display()))
}

fn board(args: &cli::Cli) -> Result<String, String> {
    let repos = resolve_repos(args)?;
    let gh_bin = args.gh_bin.clone();
    let mut statuses: Vec<model::RepoStatus> = thread::scope(|s| {
        let handles: Vec<_> = repos
            .iter()
            .map(|r| s.spawn(|| collector::collect(r, &gh_bin)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    statuses.sort_by(|a, b| b.commit_ts.cmp(&a.commit_ts));
    Ok(if args.json {
        render::json(&statuses)
    } else if args.md {
        render::markdown(&statuses)
    } else {
        render::term(&statuses)
    })
}

fn run() -> Result<String, String> {
    let args = cli::Cli::parse();
    match &args.command {
        Some(cli::Command::Next { repo, text }) => {
            let path = resolve_repo(repo, &args)?;
            next::add(&path, text).map_err(|e| format!("cannot write NEXT.md: {e}"))?;
            Ok(format!("added to {repo}/NEXT.md: {text}\n"))
        }
        Some(cli::Command::Done { repo, n }) => {
            let path = resolve_repo(repo, &args)?;
            let done = next::mark_done(&path, *n)?;
            Ok(format!("done in {repo}: {done}\n"))
        }
        None => board(&args),
    }
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

- [ ] **Step 3: Build and run the unit suite**

Run:
```bash
cargo build 2>&1 | tail -5
cargo test 2>&1 | tail -8
```
Expected: clean build; all 33 tests still pass (wiring only, no test changes this task).

- [ ] **Step 4: End-to-end smoke test against a throwaway repo**

Run:
```bash
TMP=$(mktemp -d)
git -C "$TMP" init -q -b main
git -C "$TMP" -c user.email=t@t.com -c user.name=t commit -q --allow-empty -m init
printf '# roadmap\n' > "$TMP/ROADMAP.md"
./target/debug/wip next "$TMP" "finish the thing"
./target/debug/wip next "$TMP" "write docs"
echo "--- NEXT.md ---"; cat "$TMP/NEXT.md"
echo "--- board ---"; ./target/debug/wip --root "$(dirname "$TMP")" 2>&1 | grep -A3 "$(basename "$TMP")"
./target/debug/wip done "$TMP" 1
echo "--- after done ---"; cat "$TMP/NEXT.md"
rm -rf "$TMP"
```
Expected: `NEXT.md` gets a `# Next` header + two `- [ ] ` lines; the board block for the temp repo shows `next: 1. finish the thing   2. write docs` and `see: ROADMAP.md`; after `done ... 1` the first line is `- [x] finish the thing`, the second stays open. Confirmation lines `added to .../NEXT.md: ...` and `done in ...: finish the thing` print, no panic.

- [ ] **Step 5: Test the not-found error path**

Run: `./target/debug/wip next definitely-not-a-repo "x"; echo "exit=$?"`
Expected: stderr `wip: repo 'definitely-not-a-repo' not found in config ...` and `exit=1`.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: wip next / wip done subcommands writing NEXT.md"
```

---

### Task 6: Update README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the `## Usage` block in `README.md`**

Find this block:

```markdown
## Usage

```bash
wip                 # terminal table, most recently committed repo first
wip --md            # markdown digest (for Claude Code / SessionStart hook)
wip --json          # structured output
wip --root ~/some/dir   # ad-hoc: scan immediate subdirs for git repos, ignore config
```

`gh` is optional - if it's missing or you're not authed, PR/issue columns show `-`
and git status still works.
```

Replace it with:

```markdown
## Usage

```bash
wip                 # terminal table, most recently committed repo first
wip --md            # markdown digest (for Claude Code / SessionStart hook)
wip --json          # structured output
wip --root ~/some/dir   # ad-hoc: scan immediate subdirs for git repos, ignore config
```

`gh` is optional - if it's missing or you're not authed, PR/issue columns show `-`
and git status still works.

### Next-actions

Each repo can carry a `NEXT.md` at its root - a plain markdown task list that
travels with the code via git, so your next steps are wherever the repo is. The
board shows open items numbered per repo.

```bash
wip next <repo> "finish the thing"   # append "- [ ] finish the thing" to <repo>/NEXT.md
wip done <repo> 1                    # flip the 1st OPEN item to "- [x]"
```

`<repo>` is a config basename (e.g. `wip next web-app "..."`) or a path to a repo.
`wip` writes `NEXT.md` but never commits it - commit it with the rest of your work.

The board also surfaces a `see:` line listing common planning files it finds in a
repo root (`ROADMAP.md`, `TODO.md`, `PLAN.md`, `BACKLOG.md`) - filenames only, as a
pointer to read for deeper context. It does not parse their content.
```

- [ ] **Step 2: Replace the `## Roadmap` block in `README.md`**

Find:

```markdown
## Roadmap

- **v1 (done):** read-only status across a curated repo list.
- **v2:** active management - `wip next <repo> "<next step>"` stores next-actions in
  the [`exchange`](../exchange) mailbox (`kind=next`), and `wip` renders a "next" line.
  Shared store means you and Claude Code see each other's notes.
```

Replace it with:

```markdown
## Roadmap

- **v1 (done):** read-only status across a curated repo list.
- **v2 (done):** active management - `wip next` / `wip done` manage per-repo
  next-actions in each repo's `NEXT.md`, plus a `see:` pointer to detected planning
  files, all shown on the board.
- **v3 (planned):** a SessionStart hook so Claude Code auto-runs `wip --md` at the
  start of a session, plus prebuilt cross-platform binaries on GitHub Releases.
```

- [ ] **Step 3: Verify the README renders sanely**

Run: `sed -n '/## Usage/,/v3 (planned)/p' README.md`
Expected: the new Usage (incl. `### Next-actions` and the `see:` paragraph) and the full Roadmap appear, no broken fenced blocks, no emoji.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: document wip next / wip done + planning pointers; mark v2 done"
```

---

## Self-Review

**Spec coverage:**
- NEXT.md storage + markdown task-list format + parse rules (open `- [ ]` / done `- [x]` / ignore others / open-only numbering) → Task 1 `next.rs` + tests. ✓
- done = flip in place → Task 1 `mark_done`. ✓
- `wip next <repo> "text"` (append, create-if-absent) → Task 1 `add` + Task 5 routing. ✓
- `wip done <repo> <n>` (flip n-th open, out-of-range error) → Task 1 `mark_done` + Task 5 routing. ✓
- `<repo>` resolution (existing dir → path; else basename in config; else error) → Task 5 `resolve_repo`. ✓
- `<n>` = board display number (open-only): `read_open` open-in-order (T1), renderers number 1..N (T4), `mark_done` counts open 1..N (T1) — consistent. ✓
- planning-file pointers (detect names only, case-insensitive, sorted, no content parse) → Task 2 `planning.rs` + tests. ✓
- `RepoStatus.planning_docs` field added → Task 2 model edit. ✓
- collector fills `next_actions` + `planning_docs` → Task 3. ✓
- board renders numbered next-actions + `see:` line (term + md); json carries both automatically → Task 4. ✓
- no auto-commit / no git side-effects → Task 1 + Task 5 only `fs::write`; smoke test never commits. ✓
- no new dependencies → Cargo.toml untouched across all tasks. ✓
- README updated (repo-change rule) → Task 6. ✓
- out of scope (hook, prebuilt binary, content parsing) → README roadmap v3 + not built. ✓

**Placeholder scan:** No TBD/TODO; every code step shows full code; every run step gives command + expected output. ✓

**Type consistency:** `next::read_open(&Path)->Vec<String>`, `next::add(&Path,&str)->io::Result<()>`, `next::mark_done(&Path,usize)->Result<String,String>` (T1) match calls in collector (T3) and main (T5). `planning::detect(&Path)->Vec<String>` (T2) matches collector (T3). `RepoStatus.next_actions`/`planning_docs: Vec<String>` (T2 model) used by collector (T3) and renderers (T4). `cli::Command::Next{repo,text}` / `Done{repo,n}` (T5 cli.rs) match the match arms in main (T5). Module block in final main.rs (T5) lists all of: cli, collector, config, gh, git, model, next, planning, progress, render. ✓
