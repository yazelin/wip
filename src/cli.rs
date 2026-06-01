use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "wip", about = "Cross-repo dev status for humans and AI agents")]
pub struct Cli {
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
