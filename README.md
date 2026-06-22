# wip - cross-repo dev status

One command to see where every project stands: current branch, last commit,
dirty/unpushed state, open PRs/issues, and the tail of each repo's `progress.md`.
Built so **both you and Claude Code read the same source of truth** - humans get a
terminal table, agents run `wip --md` and ingest the markdown.

## Install

### From a release (no Rust needed)

Download the archive for your OS from the [Releases](https://github.com/yazelin/wip/releases)
page, extract the `wip` binary, and put it on your PATH:

```bash
# Linux x86_64
tar xzf wip-*-linux-x86_64.tar.gz
install -m 755 wip ~/.local/bin/wip       # ensure ~/.local/bin is on PATH

# then point it at your repos
mkdir -p ~/.config/wip
cp repos.example.toml ~/.config/wip/repos.toml   # edit the list

# optional: auto-load status into Claude Code at session start
wip install-hook
```

Windows: unzip `wip-*-windows-x86_64.zip` and put `wip.exe` somewhere on your PATH.

`wip install-hook` records the absolute path of whichever `wip` binary you ran it
from, so the release binary you place on PATH is exactly the one the hook invokes.

### From source

```bash
cargo install --git https://github.com/yazelin/wip   # or: cargo install --path .
mkdir -p ~/.config/wip
cp repos.example.toml ~/.config/wip/repos.toml
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

### Claude Code integration

Let Claude Code load your cross-repo status automatically at the start of every
session:

```bash
wip install-hook          # adds a SessionStart hook to ~/.claude/settings.json AND ~/.codex/hooks.json
wip install-hook --print  # or: print the snippets to add manually, change nothing
```

`install-hook` installs into **both Claude Code and Codex** (idempotent): it backs
up each file (`.bak`) first, skips if a wip hook is already present, creates the
`~/.claude` / `~/.codex` dirs if missing, and preserves your other settings and
hooks. The hook runs `wip hook`, which prints the board as markdown with `gh`
skipped (no network, fast session starts) and stays silent if no repos are
configured.

> **Codex needs one manual step:** Codex skips untrusted hooks, so after
> `install-hook` run `/hooks` once in an interactive Codex session to trust the
> entry (re-trust if the hook command ever changes). `install-hook` prints this
> reminder. `--dangerously-bypass-hook-trust` does **not** cover a new hook.

### As a skill (Claude + Codex)

The hook pushes status passively at session start. To also teach agents wip's
command surface on demand — and to cover Codex, which the hook doesn't reach —
install wip as a skill:

```bash
wip install-skill   # writes ~/.claude/skills/wip/SKILL.md and ~/.codex/skills/wip/SKILL.md
```

The skill definition is embedded in the binary (no repo needed at runtime) and
install is idempotent. Agents that read it learn to run `wip --md` for a fresh
snapshot and `wip next` / `wip done` to manage a repo's NEXT.md.

`wip --no-gh` is also available directly for a fast, network-free board.

## Web views

Two optional HTML companions for when a terminal table isn't enough. Both open
straight from disk (`file://`, no server) - just open the file in your browser.

### Status dashboard (`dashboard.py`)

`dashboard.py` runs `wip --json`, inlines the result into a single self-contained
`~/wip-dashboard.html`, and prints its path. One card per repo, sorted most-recent
first, with a left color bar by staleness (green < 7d, amber < 30d, grey older,
red on error) and badges for dirty/unpushed/PR counts. Data is embedded, so it
works over `file://` with no server or CORS. Re-run to refresh.

```bash
python3 dashboard.py            # passes --no-gh by default
python3 dashboard.py            # any extra args are forwarded to `wip --json`
```

### Relationship graph (DIY)

There's no auto-generated dependency graph: separate repos rarely share code, so a
useful "how do these relate" picture is hand-curated, not derived. The lazy way is
a standalone HTML that pulls [Mermaid](https://mermaid.js.org/) from a CDN and
renders a `flowchart` you edit by hand - group repos into `subgraph` clusters and
draw only the edges you can vouch for (solid = real link, dashed = planned):

```html
<pre class="mermaid">
flowchart TB
  subgraph PLATFORM
    platform["platform"]
  end
  subgraph APPS
    appA["app-a"]
    appB["app-b"]
  end
  appA -->|"consumes"| platform
  appB -.->|"planned"| platform
</pre>
<script type="module">
  import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs';
  mermaid.initialize({ startOnLoad: true, theme: 'dark' });
</script>
```

## Roadmap

- **v1 (done):** read-only status across a curated repo list.
- **v2 (done):** active management - `wip next` / `wip done` manage per-repo
  next-actions in each repo's `NEXT.md`, plus a `see:` pointer to detected planning
  files, all shown on the board.
- **v3a (done):** `wip install-hook` adds a Claude Code SessionStart hook that
  auto-runs `wip hook` (markdown, `--no-gh`) at session start.
- **v3b (planned):** prebuilt cross-platform binaries on GitHub Releases + CI.

## Output has no emoji

By preference, all output is plain ASCII (`[3 dirty]`, `PR: none`) - no emoji or
symbol glyphs.
