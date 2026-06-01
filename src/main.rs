mod cli;
mod collector;
mod config;
mod gh;
mod git;
mod hook;
mod model;
mod next;
mod planning;
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

/// Resolve a `<repo>` argument to a path: an existing directory is used directly,
/// otherwise match by basename against the configured repo list.
fn resolve_repo(name: &str, args: &cli::Cli) -> Result<PathBuf, String> {
    let direct = PathBuf::from(name);
    if direct.is_dir() {
        return Ok(direct);
    }
    let cfg_path = args
        .config
        .clone()
        .unwrap_or_else(config::default_config_path);
    let repos = config::load_from_file(&cfg_path)?;
    repos
        .into_iter()
        .find(|r| r.file_name().and_then(|s| s.to_str()) == Some(name))
        .ok_or_else(|| format!("repo '{name}' not found in config {}", cfg_path.display()))
}

fn collect_sorted(args: &cli::Cli, use_gh: bool) -> Result<Vec<model::RepoStatus>, String> {
    let repos = resolve_repos(args)?;
    let gh_bin = args.gh_bin.clone();
    let mut statuses: Vec<model::RepoStatus> = thread::scope(|s| {
        let handles: Vec<_> = repos
            .iter()
            .map(|r| s.spawn(|| collector::collect(r, &gh_bin, use_gh)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    statuses.sort_by(|a, b| b.commit_ts.cmp(&a.commit_ts));
    Ok(statuses)
}

fn board(args: &cli::Cli) -> Result<String, String> {
    let statuses = collect_sorted(args, !args.no_gh)?;
    Ok(if args.json {
        render::json(&statuses)
    } else if args.md {
        render::markdown(&statuses)
    } else {
        render::term(&statuses)
    })
}

fn run() -> Result<String, String> {
    let args = cli::Cli::parse();
    match &args.command {
        Some(cli::Command::Next { repo, text }) => {
            let path = resolve_repo(repo, &args)?;
            next::add(&path, text).map_err(|e| format!("cannot write NEXT.md: {e}"))?;
            Ok(format!("added to {}/NEXT.md: {text}\n", path.display()))
        }
        Some(cli::Command::Done { repo, n }) => {
            let path = resolve_repo(repo, &args)?;
            let done = next::mark_done(&path, *n)?;
            Ok(format!("done in {}: {done}\n", path.display()))
        }
        None => board(&args),
    }
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
