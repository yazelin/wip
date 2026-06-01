# wip — 跨 repo 開發狀態 CLI(設計)

日期:2026-06-01
作者:yazelin × Claude

## 問題

yazelin 同時在開發 30+ 個 repo(mori-universe 11 個 + agentos / smriti / 部落格 …)。
追進度的方式是「進個別 repo 讀 git log」,沒有單一聚合視圖,於是常「忘記某條線做到哪」
(這次的觸發點:mori↔agentos 整合做到一半,忘了停在哪)。

需要一個**跨 repo 的開發狀態看板**,而且要**人和 Claude Code 都能用同一份事實來源**。

## 關鍵約束(已拍板)

| 決定 | 拍板 | 理由 |
|---|---|---|
| 形式 | **CLI**,不是 TUI / web app | Claude Code 讀不到 TUI 畫面,但能跑指令讀 stdout。CLI 才能人 + agent 共用 |
| 輸出 | 預設 terminal 表格;`--md`(給 agent 吞);`--json`(給程式 / v2) | 同一引擎、三種外殼 |
| 實作 | **Rust 單一 binary** | 跟 RTK / AgentPulse 生態一致、跨機(Linux+Windows)、Claude 在哪台都能可靠叫同一支、v2 接 exchange 乾淨 |
| repo 範圍 | **curated 清單(config)**,非自動掃全部 | 30+ repo 會噪音爆炸;只追 active 的 |
| 住哪 | **新開獨立 github repo `wip`** | 管的範圍超出 mori-universe(也含 agentos / smriti / blog),不塞進任何既有 repo |
| 共享狀態(v2) | 走現成 **`exchange`** 信箱 | 人和 Claude 都讀得到;不另造儲存,避免又一個過時點 |

非目標(YAGNI):不做 web UI、不做多人協作、不自動掃全機 git repo、v1 不碰 AgentPulse session 事件。

## 架構(三個獨立單元)

```
config(repo 清單) ──▶ collector(git + gh + progress.md) ──▶ 中間結構 ──▶ renderer(term / md / json)
```

### 1. config — repo 清單
- 設定檔列出要追的 repo 路徑(curated)。種子放現在 active 的:
  `mori-meeting-recorder`、`agentos`、`agentos-notebook`、`annuli`、`mori-desktop`、`mori-ear`、`smriti`、`exchange`、`yazelin.github.io`。
- 位置:`~/.config/wip/repos.toml`(跨機可放 yaze-journal 同步)。
- `wip --root <dir>`:臨時掃某資料夾底下所有 git repo,不動 config。
- **介面**:輸入 = 無 / config 路徑;輸出 = repo 路徑清單。可獨立測。

### 2. collector — 對每個 repo 收狀態
對清單裡每個 repo 平行收集:
- **git**:當前 branch、最後 commit(相對時間 + 訊息首行)、髒檔數(`git status --porcelain`)、unpushed commit 數(`@{u}..HEAD`)。
- **gh**:`gh pr list`(open PR:編號 + 標題)、`gh issue list`(open 數量)。gh 不在 / 沒登入 / 非 GitHub remote → 該欄標 `—`,不報錯。
- **progress.md**:若 repo 根目錄或常見路徑(`docs/`、notebook 子資料夾)有 `progress.md`,抓最後一段(最近的進度條目)摘要。
- **介面**:輸入 = repo 路徑;輸出 = 一個 `RepoStatus` 結構。每個 repo 獨立、可平行、可單測。失敗(非 git / 權限)→ 該 repo 標錯誤但不中斷整體。

### 3. renderer — 呈現
- **term(預設)**:人類好讀,**最近動過的排最上**。每 repo 一塊:repo · branch · 最後 commit(時間+訊息)· ⚠ 髒/unpushed · open PR · progress 摘要末行。
- **`--md`**:同資料的 markdown,給 Claude Code 在 SessionStart hook / 「我做到哪了」時吞進 context。
- **`--json`**:`RepoStatus[]` 序列化,給 v2 / 外部程式。
- **介面**:輸入 = `RepoStatus[]` + 格式 flag;輸出 = 字串。純函式、好測。

## 資料結構(中間層)

```
RepoStatus {
  name: String,
  path: String,
  branch: String,
  last_commit: { rel_time, message, sha },
  dirty_files: u32,
  unpushed: u32,
  open_prs: Vec<{ number, title }>,   // gh 不可用 → 空 + flag
  open_issues: Option<u32>,
  progress_tail: Option<String>,      // progress.md 最後一段
  next_actions: Vec<String>,          // v2 才填,來自 exchange
  error: Option<String>,
}
```

## v1 / v2 切線

- **v1(讀回記憶,本次目標)**:上述整套,唯讀。`next_actions` 永遠空。解掉「做到哪」。
- **v2(主動管理)**:
  - 讀:collector 多一步,對每個 repo `exchange pull` 抓 `to=<repo>` 且 `kind=next` 的訊息 → 填 `next_actions`。renderer term/md 多顯示「下一步」。
  - 寫:`wip next <repo> "<下一步>"` → 包成 `exchange push --to <repo> --kind next --summary "…" --ref <branch>`。人跟 Claude 互相看得到。
  - **不另造儲存**;沿用 exchange 7 欄 envelope(`id/ts/from/to/kind/summary/ref`)。

## 錯誤處理

- 單一 repo 出錯(非 git repo / 路徑不存在 / 權限)→ 該 repo 標 `error`,其餘照常,整體不中斷。
- `gh` 缺席或未登入 → PR/issue 欄標 `—`,git 狀態照常出。
- v2:exchange 連不到 → next-action 欄空 + 一行 warning(沿用 exchange 既有 2.5s fast-fail,不卡 terminal)。

## 測試

- collector:對一個受控 fixture repo(已知 branch / 髒檔 / commit)斷言 `RepoStatus`。
- renderer:餵固定 `RepoStatus[]`,斷言 term / md / json 輸出(snapshot)。
- config:解析 `repos.toml` + `--root` 掃描,斷言 repo 清單。
- gh 缺席 path:mock gh 不存在,斷言 PR 欄降級不報錯。

## 開放項(交給 implementation plan / 使用時定)

- config 預設清單最終定版(種子已給,可調)。
- progress.md 的「最後一段」如何切(用 `## ` heading 還是空行分段)——實作時看實際檔案決定。
- term 排序的「最近動過」依據:最後 commit 時間 vs 工作目錄 mtime(傾向 commit 時間)。
