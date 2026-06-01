# wip v3b — prebuilt binaries + CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add GitHub Actions CI (test + build on Ubuntu + Windows) and a tag-triggered release workflow that publishes Linux x64 + Windows x64 prebuilt `wip` binaries as a draft GitHub Release, plus README install-from-release docs.

**Architecture:** Two workflow files under `.github/workflows/` — `ci.yml` (push/PR to main) and `release.yml` (tag `v*`). Pure-Rust crate means no system deps and no tauri CLI: just `cargo test` / `cargo build --release`. No macOS. README gains an "Install from a release" section tying the binary to the v3a hook flow.

**Tech Stack:** GitHub Actions; `actions/checkout@v5`, `dtolnay/rust-toolchain@stable`, `swatinem/rust-cache@v2`, `softprops/action-gh-release@v2`. No code changes, no new crates.

**IMPORTANT — this is CI/infra, not testable code.** Per-task validation is: the YAML parses (`python3 -c 'import yaml,sys; yaml.safe_load(...)'`) and, for release, a local dry-run of the Linux build+package commands. The workflows themselves only execute on GitHub. **Live acceptance (push to main → CI; push tag `v0.1.0` → release) is a post-merge step, described at the end — it is NOT a subagent task.** Build on branch `feat/v3b-release-ci` (create it off `main` first).

---

### Task 1: CI workflow (test + build on Ubuntu + Windows)

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Create `.github/workflows/ci.yml` with this EXACT content**

```yaml
name: CI

# Build + test check on push / PR to main. Ubuntu + Windows only (no macOS —
# private repo, macOS runners bill 10x). Catches breakage incl. Windows path bugs.

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:

jobs:
  test:
    strategy:
      fail-fast: false
      matrix:
        platform: [ubuntu-22.04, windows-latest]
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@stable
      - uses: swatinem/rust-cache@v2
      - name: Test
        run: cargo test
      - name: Build
        run: cargo build --release
```

- [ ] **Step 2: Verify the YAML parses**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('ci.yml OK')"
```
Expected: `ci.yml OK` (no exception).

- [ ] **Step 3: Sanity-check the commands it runs work locally**

Run: `cargo test 2>&1 | tail -2 && cargo build --release 2>&1 | tail -2`
Expected: tests pass (40), release builds clean — confirming the exact commands the CI invokes succeed on this machine.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: test + build on ubuntu + windows (push/PR to main)"
```

---

### Task 2: Release workflow (Linux + Windows binaries, draft release)

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Create `.github/workflows/release.yml` with this EXACT content**

```yaml
name: Release

# Tag-triggered release: builds Linux + Windows binaries, archives each, and
# attaches them to a DRAFT GitHub Release (review, then publish manually).
# No macOS (private repo cost). Trigger:
#   git tag v0.1.0 && git push origin v0.1.0

on:
  push:
    tags: ['v*']
  workflow_dispatch:

permissions:
  contents: write

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: ubuntu-22.04
            name: linux-x86_64
          - platform: windows-latest
            name: windows-x86_64
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@stable
      - uses: swatinem/rust-cache@v2
        with:
          key: ${{ matrix.name }}

      - name: Build
        run: cargo build --release

      - name: Package (Linux)
        if: runner.os == 'Linux'
        shell: bash
        run: tar czf "wip-${{ github.ref_name }}-${{ matrix.name }}.tar.gz" -C target/release wip

      - name: Package (Windows)
        if: runner.os == 'Windows'
        shell: pwsh
        run: Compress-Archive -Path target\release\wip.exe -DestinationPath "wip-${{ github.ref_name }}-${{ matrix.name }}.zip"

      - name: Upload to draft release
        uses: softprops/action-gh-release@v2
        with:
          files: wip-${{ github.ref_name }}-*
          draft: true
          generate_release_notes: true
```

- [ ] **Step 2: Verify the YAML parses**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml')); print('release.yml OK')"
```
Expected: `release.yml OK`.

- [ ] **Step 3: Dry-run the Linux build + package locally**

This reproduces exactly what the Linux matrix job does, then confirms the archive contains a runnable `wip`:
```bash
cargo build --release 2>&1 | tail -2
tar czf /tmp/wip-v0.0.0-test-linux-x86_64.tar.gz -C target/release wip
echo "--- archive contents ---"; tar tzf /tmp/wip-v0.0.0-test-linux-x86_64.tar.gz
mkdir -p /tmp/wiptest && tar xzf /tmp/wip-v0.0.0-test-linux-x86_64.tar.gz -C /tmp/wiptest
/tmp/wiptest/wip --help 2>&1 | head -3
rm -rf /tmp/wiptest /tmp/wip-v0.0.0-test-linux-x86_64.tar.gz
```
Expected: archive lists exactly `wip`; the extracted binary runs (`--help` prints the usage banner). (The Windows package step uses `Compress-Archive` and can only be verified on the live runner.)

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: tag-triggered release of linux + windows binaries (draft)"
```

---

### Task 3: README — install from a release

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the `## Install` block**

Find this exact block:
```markdown
## Install

```bash
cargo install --path .          # installs `wip` into ~/.cargo/bin
mkdir -p ~/.config/wip
cp repos.example.toml ~/.config/wip/repos.toml   # then edit the list
```
```

Replace it with:
```markdown
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
```

- [ ] **Step 2: Verify the section renders and fences balance**

Run:
```bash
sed -n '/## Install/,/## Usage/p' README.md
grep -c '```' README.md
```
Expected: the new "From a release" + "From source" subsections appear; the backtick-fence count is **even**; no emoji.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: install-from-release instructions (binary is hook-ready)"
```

---

## Live acceptance (post-merge — controller runs this, NOT a subagent task)

The workflows only execute on GitHub. After this branch is merged to `main` and pushed:

1. **CI:** the push to `main` triggers `ci.yml`. Watch it:
   ```bash
   gh run watch "$(gh run list --workflow=ci.yml --limit 1 --json databaseId -q '.[0].databaseId')"
   ```
   Expected: both `ubuntu-22.04` and `windows-latest` jobs green.

2. **Release:** cut the first tag (only after CI is green):
   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   gh run watch "$(gh run list --workflow=release.yml --limit 1 --json databaseId -q '.[0].databaseId')"
   ```
   Expected: both build jobs green; `gh release list` shows a **draft** `v0.1.0` with two assets (`wip-v0.1.0-linux-x86_64.tar.gz`, `wip-v0.1.0-windows-x86_64.zip`). Publish manually via the Releases page when satisfied.

If a live job fails, read the log (`gh run view --log-failed <id>`), fix the workflow on a branch, and re-run — do not leave a broken workflow on main.

---

## Self-Review

**Spec coverage:**
- `ci.yml`: Ubuntu + Windows, test + build, push/PR to main + dispatch → Task 1. ✓
- `release.yml`: Linux + Windows only (no macOS), tag `v*` + dispatch, `contents: write`, tar.gz (Linux) / zip via Compress-Archive (Windows), softprops draft + `generate_release_notes` → Task 2. ✓
- `ubuntu-22.04` runner (glibc back-compat) → Tasks 1 & 2. ✓
- README install-from-release + `cargo install --git` from-source + hook-ready note (download → PATH → `install-hook`) → Task 3. ✓
- Acceptance: local YAML-parse + Linux package dry-run (Tasks 1-2) + post-merge live tag run (Live acceptance section). ✓
- Out of scope (macOS, signing, installers, auto-publish, CHANGELOG) → none built; draft + auto-notes only. ✓
- No code changes / no new crates → only `.github/workflows/*` + README touched. ✓

**Placeholder scan:** No TBD/TODO. Every file step shows full content; every run step has a command + expected output. The Windows package step is explicitly noted as live-runner-only (not a local-verifiable gap, an honest limitation). ✓

**Consistency:** Archive names `wip-${{ github.ref_name }}-linux-x86_64.tar.gz` / `-windows-x86_64.zip` match the `matrix.name` values (`linux-x86_64`, `windows-x86_64`) and the `files: wip-${{ github.ref_name }}-*` glob in the upload step. Action versions match across both workflows (checkout@v5, rust-toolchain@stable, rust-cache@v2). The README Releases URL and `cargo install --git` URL use the real origin `https://github.com/yazelin/wip`. ✓
