# wip v3b — prebuilt binaries + CI(設計)

日期:2026-06-01
作者:yazelin × Claude

## 問題

v3a 讓 Claude 開場自動用 wip,但「任意機器好裝」還沒解:目前唯一安裝法是 `cargo install`(要 Rust toolchain)。要能在沒有 Rust 的機器上下載一個 binary 就用,並讓 main 壞了能自動抓到。

## 關鍵決策(已拍板)

| 決定 | 拍板 | 理由 |
|---|---|---|
| release 平台 | **Linux x64 + Windows x64**(無 macOS) | 只出 yazelin 實際在用的平台;個人工具不為用不到的平台做。將來真要 macOS 再加 |
| CI 平台 | **Ubuntu + Windows**(無 macOS) | push/PR 快回饋 + 抓 Windows path bug;省 macOS runner 10× 分鐘(repo 是 private,Actions 計費) |
| release 觸發 | tag `v*`(+ `workflow_dispatch`) | 慣例;偶發、可控 |
| CI 觸發 | push / PR to `main`(+ dispatch) | main 常綠 |
| 打包格式 | Linux → `tar.gz`、Windows → `zip` | 各平台慣例 |
| release notes | `generate_release_notes: true`(自動,從 commits) | 不維護 CHANGELOG.md,零負擔 |
| 發佈方式 | **draft release**(softprops/action-gh-release@v2) | 人工 review 後再 publish,不自動公開 |
| 與 hook 關係 | release binary **本身就 hook-ready** | `wip install-hook` 用 `current_exe()` 寫絕對路徑;下載→放 PATH→`install-hook` 即指向該 binary。無獨立 hook binary |
| runner | `ubuntu-22.04`(非 24.04) | glibc 後向相容(舊系統也能跑),與 AgentPulse 一致 |

非目標(YAGNI):macOS、linux aarch64、code signing、安裝包(.deb/.msi/.dmg)、Homebrew/cargo-binstall 索引、自動 publish(維持 draft)、CHANGELOG.md。**無新增程式碼、無新增 crate**——只加兩個 workflow + README。

## 架構(三個檔)

### 1. `.github/workflows/ci.yml`
- on:`push`/`pull_request` to `main` + `workflow_dispatch`。
- matrix:`ubuntu-22.04`、`windows-latest`(`fail-fast: false`)。
- steps:`actions/checkout@v5` → `dtolnay/rust-toolchain@stable` → `swatinem/rust-cache@v2` → `cargo test` → `cargo build --release`。
- 純 Rust:**不需** apt 系統依賴、不需 tauri-cli。

### 2. `.github/workflows/release.yml`
- on:`push` tags `v*` + `workflow_dispatch`;`permissions: contents: write`。
- matrix(`fail-fast: false`):
  - `ubuntu-22.04` / name `linux-x86_64`:`cargo build --release` → `tar czf wip-${tag}-linux-x86_64.tar.gz -C target/release wip`。
  - `windows-latest` / name `windows-x86_64`:`cargo build --release` → zip `wip-${tag}-windows-x86_64.zip`(內含 `wip.exe`,用 `7z a -tzip` 或 PowerShell `Compress-Archive`)。
- 每個 matrix job 跑完上傳自己的 archive 到同一個 draft release:`softprops/action-gh-release@v2`,`draft: true`,`generate_release_notes: true`,`files: <archive>`。
- tag 名取自 `github.ref_name`。

### 3. `README.md` — 安裝段
- 新增「Install from a release」:到 Releases 下載對應 OS 的 archive → 解壓 → 把 `wip` 放進 PATH(例 `~/.local/bin`)→(可選)`wip install-hook` 接上 Claude Code。明確寫出 download → PATH → `install-hook` 這條 hook-ready 流程。
- 保留既有 `cargo install --path .` / 補 `cargo install --git <repo-url>` 當 from-source 選項。

## 驗收(誠實說明)

- **本機能驗的**:① YAML 能 parse(`python3 -c yaml.safe_load`);② Linux build+package 步驟本機 dry-run(`cargo build --release` 後 `tar czf` 出 archive、解開確認內含可執行的 `wip`)。
- **本機不能驗的**:workflow 本身只在 GitHub 上跑;Windows 打包、cross-OS、release 上傳都要實際在 Actions 上跑。
- **真正驗收 = 推第一個 tag**:`git tag v0.1.0 && git push origin v0.1.0` → `gh run watch` 看 release.yml 綠、draft release 出現、兩個 archive 掛上去。CI 則由「push 到 main」那刻自動觸發、`gh run watch` 看綠。
- 計劃會把「本機 dry-run」當 task 級驗證,把「tag 觸發 + gh run watch」當最後一步的實機驗收(需推上 GitHub)。

## 錯誤處理 / 注意

- draft release:不會自動對外公開,人工 publish。
- private repo:Actions 計費;CI 已砍掉 macOS,release 的 macOS 也砍掉,把分鐘壓到最低。
- `fail-fast: false`:一個平台掛不影響另一個,方便看哪個壞。
- hook 路徑 caveat(沿用 v3a):hook 存絕對路徑;換 binary 位置要重跑 install-hook 或先移除舊 entry(idempotency 比對任何 `…/wip" hook`,不會自動更新路徑)。README 安裝段註明。

## 開放項(實作時定)

- Windows 打包用 `7z`(GitHub windows runner 內建)還是 PowerShell `Compress-Archive`——實作時挑能跑的;`Compress-Archive` 最穩(內建、無需 7z)。
- 是否在 release archive 內附 `README.md` / `repos.example.toml`——傾向只放 binary(最小),README 在 repo 看即可。
