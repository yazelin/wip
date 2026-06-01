# wip - cross-repo dev status

One command to see where every project stands: current branch, last commit,
dirty/unpushed state, open PRs/issues, and the tail of each repo's `progress.md`.
Built so **both you and Claude Code read the same source of truth** - humans get a
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
Note: global flags must come before the subcommand, e.g.
`wip --config other.toml next web-app "..."`.

The board also surfaces a `see:` line listing common planning files it finds in a
repo root (`ROADMAP.md`, `TODO.md`, `PLAN.md`, `BACKLOG.md`) - filenames only, as a
pointer to read for deeper context. It does not parse their content.

## Roadmap

- **v1 (done):** read-only status across a curated repo list.
- **v2 (done):** active management - `wip next` / `wip done` manage per-repo
  next-actions in each repo's `NEXT.md`, plus a `see:` pointer to detected planning
  files, all shown on the board.
- **v3 (planned):** a SessionStart hook so Claude Code auto-runs `wip --md` at the
  start of a session, plus prebuilt cross-platform binaries on GitHub Releases.

## Output has no emoji

By preference, all output is plain ASCII (`[3 dirty]`, `PR: none`) - no emoji or
symbol glyphs.
