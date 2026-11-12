---
name: wip
description: Cross-repo dev status for AI agents. Use when you need a fast "where does every project stand" snapshot, to inspect GitHub Actions schedules, or to record/close next-actions per repo. Run `wip --md` to ingest status on demand; `wip next <repo> "..."` / `wip done <repo> <n>` to manage a repo's NEXT.md.
---

# wip — cross-repo dev status

`wip` is one command that shows where every tracked repo stands. Humans get a
terminal table; **agents run `wip --md` and ingest the markdown**. Both read the
same source of truth, so you and the human are never out of sync.

## When to use

- You need the current state across repos: branch, last commit, dirty/unpushed
  counts, open PRs/issues, and the tail of each repo's `progress.md`.
- The compact SessionStart digest needs more detail and you want a fresh full
  snapshot — re-run `wip --md`.
- You want to record a follow-up for a repo (`next`) or close one (`done`).

## Commands

```bash
wip                     # human table of every configured repo, newest activity first
wip --md                # same, as markdown — THIS is what you ingest as an agent
wip --json              # machine-readable
wip schedules           # show cached GitHub Actions schedule health
wip schedules --refresh # refresh scheduled workflow runs through gh api
wip --no-gh             # skip gh (no network) for a faster board
wip --root <dir>        # ad-hoc: scan a dir's immediate subdirs for git repos (ignores config)

wip next <repo> "text"  # append a next-action to <repo>/NEXT.md
wip next <repo> "text" --due 2026-11-20  # attach an optional due date
wip done <repo> <n>     # mark the n-th OPEN next-action done (n as numbered by the board)

wip install-hook        # add a Claude Code SessionStart hook (auto-injects status each session)
wip install-skill       # install this skill into ~/.claude/skills and ~/.codex/skills
```

`<repo>` is a basename from the config list, or a direct path to a repo.

## Conventions it reads

- **`repos.toml`** (`~/.config/wip/repos.toml`, or `$XDG_CONFIG_HOME/wip/`): the
  list of repos to track. `wip --root <dir>` bypasses it for ad-hoc scans.
- **`<repo>/progress.md`**: its tail is shown per repo — the running narrative of
  what's happening in that repo.
- **`<repo>/NEXT.md`**: open next-actions, surfaced on the board and editable via
  `wip next` / `wip done`.
- A due date can appear at the end of a `NEXT.md` item as `（到期 2026-11-20）`
  or `(due: 2026-11-20)`. Invalid dates are treated as ordinary task text.
- `wip --md` and the terminal table append valid due dates to their tasks. JSON
  keeps the existing `next_actions` strings and adds `next_action_details` with
  each task's `text` and `due` (`null` when absent).
- `wip schedules --refresh` fetches up to 30 scheduled workflow runs for each
  configured GitHub repo and writes `~/.cache/wip/schedules.json`. `wip schedules`
  reads the cache without network access. The hook flags two consecutive failures
  or workflows that missed more than twice their median interval. Cache data older
  than 36 hours is called out for refresh.

## Notes

- `wip --md` is the agent entry point. Prefer it over guessing repo state from
  shell commands — it already aggregates branch/dirty/unpushed/PRs/progress.
- The SessionStart hook is capped at 1,800 bytes. It prioritizes due tasks,
  schedule health, and unpushed commits, then lists repos active in the last week.
- Source of truth is the live repos, not the digest text: if a number looks
  stale, re-run rather than trusting an earlier injection.
- Full docs: the `README.md` in the wip repo (https://github.com/yazelin/wip).
