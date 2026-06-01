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

## Roadmap

- **v1 (done):** read-only status across a curated repo list.
- **v2:** active management - `wip next <repo> "<next step>"` stores next-actions in
  the [`exchange`](../exchange) mailbox (`kind=next`), and `wip` renders a "next" line.
  Shared store means you and Claude Code see each other's notes.

## Output has no emoji

By preference, all output is plain ASCII (`[3 dirty]`, `PR: none`) - no emoji or
symbol glyphs.
