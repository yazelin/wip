# wip v3a — SessionStart hook integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Claude Code auto-load cross-repo status at session start: add a `--no-gh` fast mode, a `wip hook` command that emits the board as markdown (network-free, silent on failure), and a `wip install-hook` command that idempotently adds a `SessionStart` hook to `~/.claude/settings.json` (with backup).

**Architecture:** `collector::collect` gains a `use_gh` bool (false → skip the network call via a new `gh::GhInfo::unavailable()`). `main` extracts `collect_sorted(args, use_gh)`; `board` uses `!args.no_gh`, and a new `hook_context` forces `use_gh=false` + markdown and swallows errors to an empty string. A new `src/hook.rs` does the `settings.json` read/backup/idempotent-append. Two new subcommands (`Hook`, `InstallHook { print }`).

**Tech Stack:** Rust 2021, existing deps only (clap, serde, serde_json, toml, tempfile). No new crates.

**IMPORTANT:** bin-only crate — use `cargo test <filter>` / `cargo build` / `cargo run`, NOT `cargo test --lib`. Build on branch `feat/v3a-hook` (create it off `main` first).

---

### Task 1: `gh::GhInfo::unavailable()` constructor

**Files:**
- Modify: `src/gh.rs`

- [ ] **Step 1: Add the failing test (inside the existing `mod tests` in `src/gh.rs`)**

```rust
    #[test]
    fn unavailable_constructor() {
        let g = GhInfo::unavailable();
        assert!(!g.available);
        assert!(g.prs.is_empty());
        assert!(g.open_issues.is_none());
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test gh::tests::unavailable 2>&1 | tail -15`
Expected: FAIL — `GhInfo::unavailable` does not exist (compile error).

- [ ] **Step 3: Add the constructor and reuse it in `collect`**

In `src/gh.rs`, immediately after the `pub struct GhInfo { ... }` definition (after its closing `}`), add:

```rust
impl GhInfo {
    /// The degraded result used when gh is unavailable or intentionally skipped.
    pub fn unavailable() -> GhInfo {
        GhInfo {
            available: false,
            prs: vec![],
            open_issues: None,
        }
    }
}
```

Then in `collect`, replace the fallback arm:

```rust
        _ => GhInfo {
            available: false,
            prs: vec![],
            open_issues: None,
        },
```
with:
```rust
        _ => GhInfo::unavailable(),
```

- [ ] **Step 4: Run tests**

Run: `cargo test gh 2>&1 | tail -15`
Expected: `unavailable_constructor` plus the existing gh test pass.

- [ ] **Step 5: Commit**

```bash
git add src/gh.rs
git commit -m "feat: add gh::GhInfo::unavailable() constructor"
```

---

### Task 2: `collector::collect` gains `use_gh`

**Files:**
- Modify: `src/collector.rs`

- [ ] **Step 1: Add the failing test (inside the existing `mod tests` in `src/collector.rs`)**

```rust
    #[test]
    fn use_gh_false_skips_gh() {
        let d = init_repo();
        // Even passing a real "gh" name, use_gh=false must not consult gh.
        let s = collect(d.path(), "gh", false);
        assert!(!s.gh_available);
        assert!(s.open_prs.is_empty());
        assert!(s.open_issues.is_none());
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test collector::tests::use_gh_false_skips_gh 2>&1 | tail -15`
Expected: FAIL — `collect` currently takes 2 args (compile error: this 3-arg call).

- [ ] **Step 3: Change the signature and gate the gh call**

In `src/collector.rs`, change the function signature:
```rust
pub fn collect(repo: &Path, gh_bin: &str) -> RepoStatus {
```
to:
```rust
pub fn collect(repo: &Path, gh_bin: &str, use_gh: bool) -> RepoStatus {
```

Then change:
```rust
    let gh_info = gh::collect(repo, gh_bin);
```
to:
```rust
    let gh_info = if use_gh {
        gh::collect(repo, gh_bin)
    } else {
        gh::GhInfo::unavailable()
    };
```

- [ ] **Step 4: Update the existing test call sites in this file**

The existing tests call `collect(d.path(), "wip-no-such-gh-binary-xyz")`. There are exactly two such calls (in `collects_git_repo_with_gh_unavailable` and `collects_next_actions_and_planning_docs`). Add `, true` to each so they keep exercising the real-gh path (with a bogus binary that degrades):
```rust
        let s = collect(d.path(), "wip-no-such-gh-binary-xyz", true);
```
(Apply to both occurrences. The new `use_gh_false_skips_gh` test already passes `false`.)

- [ ] **Step 5: Run tests**

Run: `cargo test collector 2>&1 | tail -15`
Expected: all collector tests pass (the two updated + the new `use_gh_false_skips_gh`).

- [ ] **Step 6: Commit**

```bash
git add src/collector.rs
git commit -m "feat: collector::collect honors use_gh (skip network when false)"
```

---

### Task 3: `--no-gh` flag + `collect_sorted` refactor in main

**Files:**
- Modify: `src/cli.rs` (add `--no-gh` flag)
- Modify: `src/main.rs` (extract `collect_sorted`, thread `use_gh` through `board`)

- [ ] **Step 1: Add the `--no-gh` flag to `src/cli.rs`**

In `src/cli.rs`, in the `Cli` struct, immediately after the `gh_bin` field:
```rust
    /// gh binary to use
    #[arg(long, default_value = "gh")]
    pub gh_bin: String,
```
add:
```rust

    /// Skip gh entirely (no network) for a faster board
    #[arg(long)]
    pub no_gh: bool,
```

- [ ] **Step 2: Refactor `board` in `src/main.rs` to use a `collect_sorted` helper**

In `src/main.rs`, replace the entire `board` function:
```rust
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
```
with:
```rust
fn collect_sorted(args: &cli::Cli, use_gh: bool) -> Result<Vec<model::RepoStatus>, String> {
    let repos = resolve_repos(args)?;
    let gh_bin = args.gh_bin.clone();
    let mut statuses: Vec<model::RepoStatus> = thread::scope(|s| {
        let handles: Vec<_> = repos
            .iter()
            .map(|r| s.spawn(|| collector::collect(r, &gh_bin, use_gh)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    statuses.sort_by(|a, b| b.commit_ts.cmp(&a.commit_ts));
    Ok(statuses)
}

fn board(args: &cli::Cli) -> Result<String, String> {
    let statuses = collect_sorted(args, !args.no_gh)?;
    Ok(if args.json {
        render::json(&statuses)
    } else if args.md {
        render::markdown(&statuses)
    } else {
        render::term(&statuses)
    })
}
```

- [ ] **Step 3: Build + full unit suite**

Run:
```bash
cargo build 2>&1 | tail -5
cargo test 2>&1 | tail -8
```
Expected: clean build; all tests pass (no test changes here — refactor + new flag).

- [ ] **Step 4: Manual smoke — `--no-gh` board is network-free**

Run: `cargo run -- --no-gh --root .. 2>&1 | head -20`
Expected: prints the board; every repo's PR column shows `-` (gh skipped), git status still present. (`collect_sorted` will be used by `wip hook` in Task 5 — it is intentionally introduced now.)

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: --no-gh flag + collect_sorted refactor"
```

---

### Task 4: `src/hook.rs` — settings.json install logic

**Files:**
- Create: `src/hook.rs`
- Modify: `src/main.rs` (add `mod hook;`)

- [ ] **Step 1: Create `src/hook.rs` with this EXACT content**

```rust
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Result of an install attempt.
pub enum Outcome {
    Installed { backup: Option<PathBuf> },
    AlreadyPresent,
}

/// Path to Claude Code's user settings: ~/.claude/settings.json
pub fn default_settings_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".claude/settings.json")
}

/// The command string the hook runs: the absolute wip binary + ` hook`.
pub fn hook_command(exe: &Path) -> String {
    format!("\"{}\" hook", exe.display())
}

/// A SessionStart entry (matcher "startup") that runs `wip hook`.
fn hook_entry(exe: &Path) -> Value {
    json!({
        "matcher": "startup",
        "hooks": [{ "type": "command", "command": hook_command(exe) }]
    })
}

/// Human-pasteable snippet for `--print`.
pub fn snippet(exe: &Path) -> String {
    format!(
        "Add this entry to hooks.SessionStart in {}:\n\n{}\n",
        default_settings_path().display(),
        serde_json::to_string_pretty(&hook_entry(exe)).unwrap()
    )
}

/// Does this command string look like a wip hook? Exact match against our
/// command, or any command ending in ` hook` that mentions wip (covers a
/// moved binary / different absolute path).
fn is_wip_hook_cmd(cmd: &str, exe: &Path) -> bool {
    if cmd == hook_command(exe) {
        return true;
    }
    let trimmed = cmd.trim_end();
    trimmed.ends_with(" hook") && trimmed.contains("wip")
}

fn already_installed(root: &Value, exe: &Path) -> bool {
    let entries = root
        .get("hooks")
        .and_then(|h| h.get("SessionStart"))
        .and_then(|v| v.as_array());
    let entries = match entries {
        Some(e) => e,
        None => return false,
    };
    for entry in entries {
        if let Some(hooks) = entry.get("hooks").and_then(|v| v.as_array()) {
            for h in hooks {
                if let Some(cmd) = h.get("command").and_then(|v| v.as_str()) {
                    if is_wip_hook_cmd(cmd, exe) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Idempotently add a SessionStart hook to `settings_path`. Backs the file up
/// (`.json.bak`) before modifying. Preserves all other keys and hooks.
pub fn install(settings_path: &Path, exe: &Path) -> Result<Outcome, String> {
    let existed = settings_path.exists();
    let mut root: Value = if existed {
        let data = std::fs::read_to_string(settings_path)
            .map_err(|e| format!("cannot read {}: {e}", settings_path.display()))?;
        if data.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&data)
                .map_err(|e| format!("malformed JSON in {}: {e}", settings_path.display()))?
        }
    } else {
        json!({})
    };

    if !root.is_object() {
        return Err(format!(
            "{} root is not a JSON object",
            settings_path.display()
        ));
    }

    if already_installed(&root, exe) {
        return Ok(Outcome::AlreadyPresent);
    }

    let backup = if existed {
        let bak = settings_path.with_extension("json.bak");
        std::fs::copy(settings_path, &bak)
            .map_err(|e| format!("cannot write backup {}: {e}", bak.display()))?;
        Some(bak)
    } else {
        None
    };

    let obj = root.as_object_mut().unwrap();
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| "hooks is not an object".to_string())?;
    let ss = hooks_obj
        .entry("SessionStart")
        .or_insert_with(|| json!([]));
    let arr = ss
        .as_array_mut()
        .ok_or_else(|| "hooks.SessionStart is not an array".to_string())?;
    arr.push(hook_entry(exe));

    let out = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? + "\n";
    std::fs::write(settings_path, out)
        .map_err(|e| format!("cannot write {}: {e}", settings_path.display()))?;
    Ok(Outcome::Installed { backup })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn exe() -> PathBuf {
        PathBuf::from("/usr/local/bin/wip")
    }

    #[test]
    fn install_creates_file_when_absent() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("settings.json");
        let outcome = install(&p, &exe()).unwrap();
        assert!(matches!(outcome, Outcome::Installed { backup: None }));
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let arr = v["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["matcher"], "startup");
        assert!(arr[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with("\" hook"));
    }

    #[test]
    fn install_preserves_existing_and_backs_up() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("settings.json");
        std::fs::write(
            &p,
            r#"{"permissions":{"x":1},"hooks":{"SessionStart":[{"matcher":"","hooks":[{"type":"command","command":"other-tool"}]}]}}"#,
        )
        .unwrap();
        let outcome = install(&p, &exe()).unwrap();
        assert!(matches!(outcome, Outcome::Installed { backup: Some(_) }));

        let bak = p.with_extension("json.bak");
        assert!(bak.exists());
        assert!(std::fs::read_to_string(&bak).unwrap().contains("other-tool"));

        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v["permissions"]["x"], 1);
        let arr = v["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert!(arr.iter().any(|e| e["hooks"][0]["command"] == "other-tool"));
    }

    #[test]
    fn install_is_idempotent() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("settings.json");
        install(&p, &exe()).unwrap();
        let after_first = std::fs::read_to_string(&p).unwrap();
        let outcome = install(&p, &exe()).unwrap();
        assert!(matches!(outcome, Outcome::AlreadyPresent));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), after_first);
    }

    #[test]
    fn install_errors_on_malformed_json_without_touching_file() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("settings.json");
        std::fs::write(&p, "{ not json").unwrap();
        assert!(install(&p, &exe()).is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ not json");
    }

    #[test]
    fn snippet_contains_abs_path_and_startup() {
        let s = snippet(&exe());
        assert!(s.contains("/usr/local/bin/wip"));
        assert!(s.contains("hook"));
        assert!(s.contains("startup"));
    }
}
```

- [ ] **Step 2: Add `mod hook;` to `src/main.rs`**

Insert `mod hook;` in the module block (alphabetical, between `git` and `model`):
```rust
mod cli;
mod collector;
mod config;
mod gh;
mod git;
mod hook;
mod model;
mod next;
mod planning;
mod progress;
mod render;
```
(`install`/`snippet`/`default_settings_path` are unused until Task 5 — expected warnings; do NOT add `#[allow(dead_code)]`.)

- [ ] **Step 3: Run tests**

Run: `cargo test hook 2>&1 | tail -20`
Expected: 5 tests pass (`install_creates_file_when_absent`, `install_preserves_existing_and_backs_up`, `install_is_idempotent`, `install_errors_on_malformed_json_without_touching_file`, `snippet_contains_abs_path_and_startup`).

- [ ] **Step 4: Commit**

```bash
git add src/hook.rs src/main.rs
git commit -m "feat: hook.rs - idempotent settings.json install with backup"
```

---

### Task 5: `wip hook` + `wip install-hook` subcommands

**Files:**
- Modify: `src/cli.rs` (add `Hook` and `InstallHook` variants)
- Modify: `src/main.rs` (add `hook_context` + the two match arms)

- [ ] **Step 1: Add the subcommands to `src/cli.rs`**

In `src/cli.rs`, in the `Command` enum, after the `Done { ... }` variant (before the enum's closing `}`), add:
```rust
    /// Print cross-repo status as markdown for a Claude Code SessionStart hook
    Hook,
    /// Install a SessionStart hook into ~/.claude/settings.json (idempotent, backs up)
    InstallHook {
        /// Print the settings.json snippet instead of editing the file
        #[arg(long)]
        print: bool,
    },
```

- [ ] **Step 2: Add `hook_context` and the match arms to `src/main.rs`**

In `src/main.rs`, add this function immediately above `fn run()`:
```rust
/// Best-effort context for the SessionStart hook: board as markdown, gh skipped,
/// framed for the agent. Any failure (no config / no repos) yields an empty
/// string so the hook never pollutes the session with errors.
fn hook_context(args: &cli::Cli) -> String {
    match collect_sorted(args, false) {
        Ok(statuses) if !statuses.is_empty() => format!(
            "Cross-repo dev status (auto-injected by wip):\n\n{}",
            render::markdown(&statuses)
        ),
        _ => String::new(),
    }
}
```

Then in `fn run()`, the `match &args.command { ... }` currently has arms for `Next`, `Done`, and `None`. Add two arms before `None => board(&args),`:
```rust
        Some(cli::Command::Hook) => Ok(hook_context(&args)),
        Some(cli::Command::InstallHook { print }) => {
            let exe =
                std::env::current_exe().map_err(|e| format!("cannot find wip's own path: {e}"))?;
            if *print {
                Ok(hook::snippet(&exe))
            } else {
                let path = hook::default_settings_path();
                match hook::install(&path, &exe)? {
                    hook::Outcome::Installed { backup } => {
                        let b = match backup {
                            Some(p) => format!(" (backup: {})", p.display()),
                            None => " (new file)".to_string(),
                        };
                        Ok(format!(
                            "installed wip SessionStart hook in {}{}\n",
                            path.display(),
                            b
                        ))
                    }
                    hook::Outcome::AlreadyPresent => Ok(format!(
                        "wip hook already present in {} (no change)\n",
                        path.display()
                    )),
                }
            }
        }
```

- [ ] **Step 3: Build + full unit suite**

Run:
```bash
cargo build 2>&1 | tail -5
cargo test 2>&1 | tail -8
```
Expected: clean build; all tests pass.

- [ ] **Step 4: Smoke — `wip hook` against a temp config**

Run:
```bash
TMP=$(mktemp -d)
git -C "$TMP/r1" init -q -b main 2>/dev/null; mkdir -p "$TMP/r1"; git -C "$TMP/r1" init -q -b main
git -C "$TMP/r1" -c user.email=t@t.com -c user.name=t commit -q --allow-empty -m init
printf '# Next\n\n- [ ] resume here\n' > "$TMP/r1/NEXT.md"
CFG="$TMP/repos.toml"; printf 'repos = ["%s"]\n' "$TMP/r1" > "$CFG"
echo "--- wip hook (has config) ---"
./target/debug/wip --config "$CFG" hook
echo "--- wip hook (missing config => must be empty, exit 0) ---"
./target/debug/wip --config /no/such/repos.toml hook; echo "exit=$?"
rm -rf "$TMP"
```
Expected: first call prints `Cross-repo dev status (auto-injected by wip):` then a markdown board including `- next:` / `1. resume here` and `PRs: - (gh unavailable)` (gh skipped); second call prints **nothing** and `exit=0`.

- [ ] **Step 5: Smoke — `wip install-hook --print` and idempotent install against a temp settings file**

Run:
```bash
TMP=$(mktemp -d); SET="$TMP/settings.json"
printf '{"hooks":{"SessionStart":[{"matcher":"","hooks":[{"type":"command","command":"other"}]}]}}' > "$SET"
echo "--- --print (no file change) ---"
./target/debug/wip install-hook --print | head -12
echo "--- HOME override install ---"
HOME="$TMP/fakehome"; mkdir -p "$HOME/.claude"; cp "$SET" "$HOME/.claude/settings.json"
HOME="$HOME" ./target/debug/wip install-hook
echo "--- second run (idempotent) ---"
HOME="$TMP/fakehome" ./target/debug/wip install-hook
echo "--- resulting SessionStart entries ---"
python3 -c "import json;d=json.load(open('$TMP/fakehome/.claude/settings.json'));print(len(d['hooks']['SessionStart']),'entries'); print('bak exists:', __import__('os').path.exists('$TMP/fakehome/.claude/settings.json.bak'))"
rm -rf "$TMP"
```
Expected: `--print` shows a JSON snippet with `"matcher": "startup"` and a `wip" hook` command, no file written; first install prints `installed ... (backup: ...)`; second prints `already present ... (no change)`; final count is **2** SessionStart entries (the original `other` + one wip), and the `.bak` exists.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: wip hook + wip install-hook subcommands"
```

---

### Task 6: README — document the hook

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add a `### Claude Code integration` subsection under `## Usage`**

In `README.md`, find the end of the `### Next-actions` subsection (it ends with the paragraph about the `see:` line and planning files). Immediately after that paragraph (before the `## Roadmap` heading), insert:

```markdown
### Claude Code integration

Let Claude Code load your cross-repo status automatically at the start of every
session:

```bash
wip install-hook        # idempotently adds a SessionStart hook to ~/.claude/settings.json
wip install-hook --print  # or: print the snippet to add manually, change nothing
```

`install-hook` backs up `settings.json` to `settings.json.bak` first, skips if a
wip hook is already present, and preserves your other settings and hooks. The
hook runs `wip hook`, which prints the board as markdown with `gh` skipped (no
network, fast session starts) and stays silent if no repos are configured.

`wip --no-gh` is also available directly for a fast, network-free board.
```

- [ ] **Step 2: Update the `## Roadmap` block**

Find:
```markdown
- **v3 (planned):** a SessionStart hook so Claude Code auto-runs `wip --md` at the
  start of a session, plus prebuilt cross-platform binaries on GitHub Releases.
```
Replace with:
```markdown
- **v3a (done):** `wip install-hook` adds a Claude Code SessionStart hook that
  auto-runs `wip hook` (markdown, `--no-gh`) at session start.
- **v3b (planned):** prebuilt cross-platform binaries on GitHub Releases + CI.
```

- [ ] **Step 3: Verify**

Run: `sed -n '/### Claude Code integration/,/v3b (planned)/p' README.md`
Expected: the new integration subsection + updated roadmap appear, fenced blocks balanced, no emoji.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: document wip install-hook / wip hook + --no-gh; mark v3a done"
```

---

## Self-Review

**Spec coverage:**
- `--no-gh` flag + collector `use_gh` (skip network) → Task 1 (`GhInfo::unavailable`) + Task 2 (`collect` signature) + Task 3 (flag + `collect_sorted`). ✓
- `wip hook`: markdown, gh skipped, framed, always exit 0, silent on no-config → Task 5 `hook_context` (forces `use_gh=false`, `Err`/empty → `String::new()`) + `Hook` arm; `main` already prints `Ok` to stdout exit 0. ✓
- plain-markdown injection (not JSON additionalContext) → `hook_context` returns plain text. ✓
- `wip install-hook`: auto-edit settings.json, `.bak` backup, idempotent, preserve other keys, abs path via `current_exe()`, `--print` escape hatch → Task 4 `hook.rs` (`install`/`snippet`/`hook_command`/`default_settings_path`) + Task 5 `InstallHook` arm. ✓
- matcher `startup` → `hook_entry` in Task 4. ✓
- malformed-JSON → Err without overwriting; backup before write → Task 4 `install` + test `install_errors_on_malformed_json_without_touching_file`. ✓
- idempotency (the anti-dupe goal) → Task 4 `already_installed` + test `install_is_idempotent`. ✓
- README updated (repo-change rule) → Task 6. ✓
- no new dependencies → Cargo.toml untouched across all tasks. ✓
- out of scope (v3b binaries, clear/compact, JSON form) → README roadmap v3b + not built. ✓

**Placeholder scan:** No TBD/TODO; every code step shows full code; every run step gives command + expected output. ✓

**Type consistency:** `gh::GhInfo::unavailable() -> GhInfo` (T1) used in collector (T2) and is the same struct used by `gh::collect`. `collector::collect(&Path, &str, bool)` (T2) is called with 3 args everywhere after: collector tests (T2), `collect_sorted` (T3). `collect_sorted(&cli::Cli, bool) -> Result<Vec<RepoStatus>, String>` (T3) is called by `board` (T3) and `hook_context` (T5). `hook::install(&Path,&Path)->Result<Outcome,String>`, `hook::snippet(&Path)->String`, `hook::default_settings_path()->PathBuf`, `hook::Outcome::{Installed{backup:Option<PathBuf>},AlreadyPresent}` (T4) match the `InstallHook` arm (T5). `cli::Command::{Hook, InstallHook{print:bool}}` (T5 cli.rs) match the arms in `run()` (T5 main.rs). `mod hook;` added in T4 before T5 uses it. ✓
