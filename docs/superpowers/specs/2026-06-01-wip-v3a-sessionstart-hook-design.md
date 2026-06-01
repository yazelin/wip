# wip v3a — SessionStart hook integration(設計)

日期:2026-06-01
作者:yazelin × Claude

## 問題

v1/v2 讓 `wip` 能給人和 Claude 一份跨 repo 狀態,但「Claude 會用它」目前只靠記憶提示,非自動。要讓 Claude **每次開 session 就自動知道所有追蹤 repo 的現況 + 各自的下一步**,需要一個 Claude Code SessionStart hook 自動跑 `wip` 並把輸出注入 context。

## Claude Code SessionStart hook 契約(已查證)

- 設定在 `~/.claude/settings.json` 的 `hooks.SessionStart`:陣列,每個 entry `{ "matcher": <source>, "hooks": [{ "type": "command", "command": "<cmd>" }] }`。
- `matcher` 值:`startup`(新 session)/ `resume` / `clear` / `compact`;空字串 `""` = 全部。
- 注入方式:hook command 的 **stdout 純文字會被當 context 附加**(或回 JSON `hookSpecificOutput.additionalContext`,本設計用純文字,較簡單且等效)。
- best-effort:**不會讓 session 失敗**;非 0 exit 不阻擋開場。但 **同步 hook 會增加開場延遲**(`wip` 的 gh 是網路呼叫 → 必須避開)。
- hook command 由 stdin 收到 JSON(cwd / session_id / source …),本設計**忽略 stdin**。

## 關鍵決策(已拍板)

| 決定 | 拍板 | 理由 |
|---|---|---|
| 注入內容 | `wip hook` 跑 board 的 markdown,**強制 --no-gh** | 避開每次開場的 gh 網路延遲;git+NEXT.md+planning 已足夠回答「做到哪/下一步」 |
| hook 失敗策略 | `wip hook` **永遠 exit 0**;無 config / 無 repo → **印空字串**(不噴錯進 context) | hook 是 best-effort,不能污染 context 或嚇到使用者 |
| 注入格式 | **純 markdown stdout**(非 JSON additionalContext) | 最簡單、契約已確認 stdout 即 context;前綴一行 framing 讓 Claude 知道這是什麼 |
| 安裝方式 | `wip install-hook` **自動編輯 settings.json**:先備份 `.bak`、冪等(已裝則跳過)、保留其他 keys/hooks;另有 `--print` 只印 snippet | 跨機一指令搞定;使用者 settings.json 有多個既有 hook,必須安全 in-place 編輯 |
| 安裝路徑 | 寫入的 command 用 `current_exe()` 的**絕對路徑** + ` hook` | 跟既有 hook 一致(絕對路徑),不依賴 PATH |
| matcher | `startup`(本次) | 「開 session 就知道」的核心情境;`clear`/`compact` 留 follow-up |

非目標(YAGNI,v3b/之後):prebuilt binary 跨平台發布、CI/Release、`clear`/`compact` matcher、JSON additionalContext 形式、`wip uninstall-hook`(可手動移或用 install 的 .bak 還原)。**無新增 dependency**(serde_json 已有)。

## 架構變動(對 v2 的增量)

### 1. `--no-gh` flag + collector 簽章
- `cli::Cli` 加 `pub no_gh: bool`(`#[arg(long)]`)。也成為使用者可用的 board flag(`wip --no-gh` = 不打網路的快板)。
- `collector::collect(repo, gh_bin, use_gh: bool)` 加一個 bool 參數。`use_gh == false` 時跳過 `gh::collect`,改用 `gh::GhInfo::unavailable()`(新增的 constructor,回 `{ available:false, prs:[], open_issues:None }`)。
- `main::board()` 傳 `!args.no_gh`。
- 影響:`collect` 的 3 個既有 test 呼叫點要加第三個引數(傳 `true` 維持原行為)。

### 2. `wip hook` 子指令
- `cli::Command::Hook`(無參數)。
- `main::run()` 的 `Hook` arm:跑 board 流程**強制 use_gh=false**、render markdown,前綴一行 framing(例:`Cross-repo dev status (auto-injected by wip):`)。
- **永遠回 `Ok`**:若 `resolve_repos` 失敗(無 config)或無 repo → 回 `Ok(String::new())`(印空、exit 0)。即把 board 的 `Err` 吞成空字串。
- 忽略 stdin。

### 3. `wip install-hook` 子指令 + `src/hook.rs`(新)
- `cli::Command::InstallHook { #[arg(long)] print: bool }`。
- `src/hook.rs`:
  - `hook_command(exe: &Path) -> String` → `format!("\"{}\" hook", exe.display())`。
  - `snippet(exe: &Path) -> String` → 給 `--print` 的 JSON entry 字串 + settings.json 路徑說明。
  - `install(settings_path: &Path, exe: &Path) -> Result<Outcome, String>`,`Outcome = Installed | AlreadyPresent`:
    1. 讀檔(不存在 → 視為 `{}`)。
    2. 解析 serde_json `Value`;非物件 → Err。
    3. 找 `hooks.SessionStart`(陣列);掃既有 entry 的 command,**若任一含 `<exe> hook` 或子字串 `wip hook` → 回 `AlreadyPresent`**(冪等,不改檔)。
    4. 否則:寫 `settings_path.with_extension("json.bak")` 備份原內容 → push `{ "matcher":"startup", "hooks":[{ "type":"command", "command": hook_command }] }` → pretty-print 寫回(保留所有其他 key/hook)。
- `main::run()`:`InstallHook { print: true }` → 印 `hook::snippet`;`print: false` → `current_exe()` → `hook::install(default_settings_path(), &exe)` → 印結果(Installed 含備份路徑 / AlreadyPresent)。
- settings.json 路徑:`~/.claude/settings.json`(用 HOME;`hook.rs` 提供 `default_settings_path()`)。

## 資料流

- **安裝(一次)**:`wip install-hook` → 備份 → append hook entry → 之後每次開 Claude session 自動跑 `wip hook`。
- **每次開場**:Claude SessionStart(startup)→ 跑 `<abs>/wip hook` → stdout markdown(--no-gh)→ 注入 context → Claude 開場即知各 repo 現況 + next-actions + 規劃檔指標。

## 錯誤處理

- `wip hook`:任何內部錯(無 config / 路徑壞 / gh 本就跳過)→ 印空字串、exit 0。永不噴 stderr 進 context。
- `wip install-hook`:settings.json 是壞 JSON → Err(印到 stderr、exit 1,**不覆寫**)。寫檔前一定先備份。冪等:重跑不會產生第二筆(直接回 AlreadyPresent)——這正是修掉「像 AgentPulse 那樣累積重複」的關鍵。
- `--no-gh` board:正常出,只是 PR/issue 欄當 gh 不可用顯示 `-`。

## 測試

- `gh::GhInfo::unavailable()`:回 available=false / 空 prs / None issues。
- `collector`:`collect(repo, gh, false)` → `gh_available == false`、`open_prs` 空(即使 gh 存在也不呼叫);`collect(repo, gh, true)` 維持原行為。
- `hook::install`:
  - 空/不存在 settings → 建立並含一筆 SessionStart wip entry;`.bak` 不會建(原本無檔)或依實作(原檔存在才備份)。
  - 既有 settings(含別的 hook)→ append 後其他 hook/key 全保留、`.bak` 內容 == 原檔。
  - 再跑一次 → `AlreadyPresent`、檔案不變(冪等)。
  - 壞 JSON → Err、原檔不動。
- `hook::snippet` / `hook_command`:含絕對路徑 + ` hook`。
- `wip hook`(整合,手動 smoke):對一個臨時 config 跑 → 印 framing + markdown;對不存在的 config 跑 → 印空、exit 0。

## 開放項(實作時定)

- framing 那行的確切文字:實作時定稿(一行、純 ASCII、無 emoji)。
- `.bak` 命名:`settings.json.bak`(若已存在是否覆蓋)——實作時採「每次安裝覆蓋同名 .bak」即可,因有 git/使用者既有多份 bak。
