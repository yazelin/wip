use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "wip", about = "跨 repo 開發進度摘要")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// 輸出 Markdown 摘要，供 AI agent 閱讀
    #[arg(long)]
    pub md: bool,

    /// 輸出 JSON 資料
    #[arg(long)]
    pub json: bool,

    /// 掃描指定目錄的第一層子目錄，尋找 Git repo（忽略設定檔）
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// 設定檔路徑（預設為 ~/.config/wip/repos.toml）
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// 指定 gh 執行檔
    #[arg(long, default_value = "gh")]
    pub gh_bin: String,

    /// 完全略過 gh，不連網以加快輸出
    #[arg(long)]
    pub no_gh: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 新增待辦至 <repo>/NEXT.md
    Next {
        /// 設定檔中的 repo 名稱，或 repo 路徑
        repo: String,
        /// 待辦文字
        text: String,
        /// 到期日，格式為 YYYY-MM-DD
        #[arg(long)]
        due: Option<String>,
    },
    /// 將 <repo>/NEXT.md 的第 n 項未完成待辦標記為完成
    Done {
        /// 設定檔中的 repo 名稱，或 repo 路徑
        repo: String,
        /// 未完成待辦的序號，從 1 開始
        n: usize,
    },
    /// 輸出供 Claude Code SessionStart hook 使用的精簡摘要
    Hook,
    /// 查看 GitHub Actions 排程健康快取
    Schedules {
        /// 更新所有設定 repo 的排程資料
        #[arg(long)]
        refresh: bool,
    },
    /// 安裝 SessionStart hook 至 Claude Code 與 Codex 設定
    InstallHook {
        /// 只列出設定片段，不修改檔案
        #[arg(long)]
        print: bool,
    },
    /// 安裝 wip skill 至 Claude Code 與 Codex
    InstallSkill,
}
