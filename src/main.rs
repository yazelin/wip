mod cli;
mod collector;
mod config;
mod gh;
mod git;
mod model;
mod next;
mod progress;
mod render;

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;

fn resolve_repos(args: &cli::Cli) -> Result<Vec<PathBuf>, String> {
    if let Some(root) = &args.root {
        let repos = config::scan_root(root);
        if repos.is_empty() {
            return Err(format!("no git repos found under {}", root.display()));
        }
        return Ok(repos);
    }
    let cfg_path = args
        .config
        .clone()
        .unwrap_or_else(config::default_config_path);
    config::load_from_file(&cfg_path).map_err(|e| {
        format!("{e}\nCreate it, e.g.: cp repos.example.toml {}", cfg_path.display())
    })
}

fn run() -> Result<String, String> {
    let args = cli::Cli::parse();
    let repos = resolve_repos(&args)?;

    // Collect repos in parallel; git+gh subprocesses dominate wall time.
    let gh_bin = args.gh_bin.clone();
    let mut statuses: Vec<model::RepoStatus> = thread::scope(|s| {
        let handles: Vec<_> = repos
            .iter()
            .map(|r| s.spawn(|| collector::collect(r, &gh_bin)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    // Most recently committed first; error/empty repos (commit_ts 0) sink to bottom.
    statuses.sort_by(|a, b| b.commit_ts.cmp(&a.commit_ts));

    let out = if args.json {
        render::json(&statuses)
    } else if args.md {
        render::markdown(&statuses)
    } else {
        render::term(&statuses)
    };
    Ok(out)
}

fn main() -> ExitCode {
    match run() {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("wip: {e}");
            ExitCode::FAILURE
        }
    }
}
