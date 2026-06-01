# wip v2 — next-actions via NEXT.md(設計)

日期:2026-06-01
作者:yazelin × Claude

## 問題

v1 解掉「每個 repo 做到哪」(讀回狀態)。v2 要加**主動管理**:記下每個 repo 的「下一步」,而且人和 Claude Code 共用同一份,讓開場就能接回未完成的工作。

## 關鍵決策(已拍板)

| 決定 | 拍板 | 理由 |
|---|---|---|
| 存哪 | **每個 repo 根目錄一個 `NEXT.md`** | 跟 code 一起 git 版控,clone 到哪台機器 next-action 就在那,零同步設定 |
| 格式 | **標準 markdown task list**(`- [ ]` 開放 / `- [x]` 完成) | 人可直接手改、git diff 友善、不需自訂 parser 規則 |
| done 模型 | **原地翻成 `- [x]`** | 真可變狀態,沒有 append-only 累積 hack;要清乾淨直接編檔 |
| 不接 exchange | v1 spec 原提案 exchange,重新評估後**整個拿掉** | mailbox 語義不合 todo、done 要 fold hack、依賴 prod box 在線;NEXT.md 更簡單自足 |
| 不自動 commit | wip 只寫 `NEXT.md`,**不碰 git** | 無 git side-effect;NEXT.md 會出現在 board 的髒檔數裡(順便提醒你 commit) |
| 規劃檔指標 | board 偵測常見規劃檔(roadmap/TODO/plan/backlog)**只列檔名、不解析內容** | wip 保持 deterministic;明確告訴 Claude「去讀這些」,而不是賭它會自己想到。roadmap 是長期方向、NEXT.md 是即時下一步、progress.md 是剛做完——三種高度分開,不混進 next 行 |

分工原則:**wip = 便宜的 deterministic 快照,把 Claude 指向 repo**(branch / 最後 commit / progress / next / 有哪些規劃檔);**Claude = 被指到後自己深讀**(roadmap、docs、diff)。wip 不負責 ingest 所有規劃內容,只負責讓 Claude 知道「去哪挖」。

非目標(YAGNI,留待 v3/之後):SessionStart hook(讓 Claude 開場自動跑 `wip --md`)、prebuilt binary 跨平台發布、`clear-done` 指令、可設定的 NEXT.md 路徑、**解析規劃檔內容**(只偵測檔名)。**無新增 dependency。**

## NEXT.md 格式

repo 根目錄 `NEXT.md`:

```markdown
# Next

- [ ] finish PeopleTab UI
- [x] wire speaker enrollment
- [ ] write tests for voiceprint matching
```

解析規則:
- 一行 trim 左空白後以 `- [ ] ` 開頭 → **開放項**,其餘為項目文字。
- 以 `- [x] ` 或 `- [X] ` 開頭 → **完成項**(忽略不顯示)。
- 其他行(heading、空行、敘述)→ 忽略。
- 開放項依檔案順序編號 1..N(**只算開放項**,這樣 board 顯示的編號 = `wip done` 用的編號)。

## 指令(3 個)

```
wip                         # board:每個 repo 多顯示「numbered 開放 next-action」
wip next <repo> "<text>"    # 在 <repo>/NEXT.md 尾端加 "- [ ] <text>"(檔不存在就建)
wip done <repo> <n>         # 把第 n 個「開放項」翻成 "- [x]"
```

`<repo>` 解析:若含路徑分隔字元或本身是存在的目錄 → 直接當路徑;否則用 basename 比對 config 的 repo 清單(如 `wip next mori-desktop "..."`)。比不到 → 報錯。

`<n>`:對應 board 顯示的編號(只在開放項間編號)。超出範圍 → 報錯。

## 架構變動(對 v1 的增量)

```
config ─▶ collector(git + gh + progress.md + NEXT.md)─▶ RepoStatus ─▶ renderer(term/md/json)
                                         ▲
              wip next / wip done ───────┘ (寫 NEXT.md)
```

### 1. `src/next.rs`(新)— NEXT.md 解析 + 變更
- `read_open(repo: &Path) -> Vec<String>`:讀 `<repo>/NEXT.md`,回開放項文字清單(檔不存在 → 空 vec,非錯)。
- `add(repo: &Path, text: &str) -> io::Result<()>`:append `- [ ] <text>\n`;檔不存在則先寫 `# Next\n\n`;檔尾沒換行先補。
- `mark_done(repo: &Path, n: usize) -> Result<String, String>`:找第 n 個開放項(1-based),把該行 `- [ ] ` 換成 `- [x] `(保留前導空白與文字),寫回;回完成的項目文字。n 超範圍 → `Err`。
- **介面**:輸入 repo 路徑 (+ text/n);輸出純資料 / io 結果。解析與 board/cli 無關,可獨立單測(temp file)。

### 1b. `src/planning.rs`(新)— 規劃檔偵測
- `detect(repo: &Path) -> Vec<String>`:讀 repo 根目錄,回存在的常見規劃檔名(case-insensitive 比對 `roadmap.md` / `todo.md` / `todo` / `plan.md` / `backlog.md`),排序。**只看檔名存在,不讀內容。** 無 → 空 vec。
- `model.rs` 隨之加一個欄位 `planning_docs: Vec<String>`(Default 空、Serialize)。

### 2. `src/collector.rs`(改)
- 在組 `RepoStatus` 時多兩步:`next_actions: next::read_open(repo)`、`planning_docs: planning::detect(repo)`。其餘不變。

### 3. `src/cli.rs`(改)
- 加 optional subcommand enum:`Next { repo: String, text: String }`、`Done { repo: String, n: usize }`。無 subcommand = board(沿用既有 md/json/root/config/gh-bin flags)。
- 加 `resolve_repo(name: &str, args) -> Result<PathBuf, String>`:依上述 `<repo>` 解析規則(路徑優先,否則 basename 比對 config)。

### 4. `src/main.rs`(改)
- `run()` 先 match subcommand:`Some(Next)` → resolve repo → `next::add` → 印確認;`Some(Done)` → resolve → `next::mark_done` → 印確認;`None` → 現有 board 流程(平行 collect → sort → render)。

### 5. `src/render.rs`(改)
- term:每 repo 區塊,若 `next_actions` 非空,多一行 `  next: 1. <text>   2. <text>`(numbered、空白分隔);若 `planning_docs` 非空,再多一行 `  see: roadmap.md, TODO.md`。
- markdown:`next_actions` 非空 → 多一段
  ```
  - next:
    1. <text>
    2. <text>
  ```
  `planning_docs` 非空 → 多一行 `- see: roadmap.md, TODO.md`。
- json:`next_actions` / `planning_docs` 已在序列化內,自動帶出。

### 6. `src/model.rs`(改)— 加 `planning_docs: Vec<String>` 欄位(`next_actions: Vec<String>` v1 已有)。

## 資料流

- **讀(board)**:`wip` → collector 對每 repo `next::read_open` → 填 `next_actions` → renderer 顯示 numbered 開放項。
- **寫(add)**:`wip next <repo> "x"` → resolve repo → `next::add` → 印 `added to <repo>/NEXT.md: x`。
- **寫(done)**:`wip done <repo> 2` → resolve repo → `next::mark_done(.., 2)` → 印 `done in <repo>: <text>`。

## 錯誤處理

- `NEXT.md` 不存在 / 讀不到 → `read_open` 回空 vec,board 照常(該 repo 無 next 行)。
- `wip next/done <repo>` repo 解析不到 → `Err(String)`,main 印 `wip: <msg>` 到 stderr、exit 1。
- `wip done <repo> <n>` n 超出開放項數 → `Err`,訊息含目前開放項數。
- `wip next` 寫檔 io 失敗 → `Err` 上拋。

## 測試

- `next::read_open`:NEXT.md 含開放/完成/heading/空行混合 → 只回開放項文字、順序正確;檔不存在 → 空 vec。
- `next::add`:檔不存在 → 建檔含 header + 項目;既有檔 → 尾端 append、不動既有行。
- `next::mark_done`:翻第 n 個開放項為 `[x]`、其餘不動、回該文字;n 超範圍 → `Err`。
- `planning::detect`:有 ROADMAP.md/TODO.md(混 README)→ 只回規劃檔名、case-insensitive、排序;無 → 空 vec。
- `cli::resolve_repo`:basename 比對命中 config;路徑直給命中;比不到 → `Err`。
- `render`:`next_actions` 非空時 term/md 出現 numbered next 行、`planning_docs` 非空時出現 see 行;空時都不出現。
- collector:NEXT.md 有開放項 + 有 ROADMAP.md 的 fixture repo → `RepoStatus.next_actions` / `planning_docs` 被填。

## 開放項(實作時定)

- `add` 的項目是否帶時間戳/branch ref:**v2 不帶**,純文字,保持 NEXT.md 乾淨可手改。
- term 的 next 行若項目很多是否截斷:v2 全列,過長再說(YAGNI)。
