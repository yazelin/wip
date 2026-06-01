use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "wip", about = "Cross-repo dev status for humans and AI agents")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Output markdown (for AI agents to ingest)
    #[arg(long)]
    pub md: bool,

    /// Output json
    #[arg(long)]
    pub json: bool,

    /// Scan a directory's immediate subdirs for git repos (ignores config)
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Config file path (default: ~/.config/wip/repos.toml)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// gh binary to use
    #[arg(long, default_value = "gh")]
    pub gh_bin: String,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Add a next-action to <repo>/NEXT.md
    Next {
        /// Repo name (basename in config) or path to a repo
        repo: String,
        /// The next-action text
        text: String,
    },
    /// Mark the n-th open next-action done in <repo>/NEXT.md
    Done {
        /// Repo name (basename in config) or path to a repo
        repo: String,
        /// 1-based index among OPEN items, as numbered by the board
        n: usize,
    },
}
