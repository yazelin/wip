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
mod skill;

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

/// Best-effort context for the SessionStart hook: board as markdown, gh skipped,
/// framed for the agent. Any failure (no config / no repos) yields an empty
/// string so the hook never pollutes the session with errors.
fn hook_context(args: &cli::Cli) -> String {
    match collect_sorted(args, false) {
        Ok(statuses) if !statuses.is_empty() => format!(
            "Cross-repo dev status (auto-injected by wip):\n\n{}",
            render::markdown(&statuses)
        ),
        _ => String::new(),
    }
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
        Some(cli::Command::Hook) => Ok(hook_context(&args)),
        Some(cli::Command::InstallHook { print }) => {
            let exe =
                std::env::current_exe().map_err(|e| format!("cannot find wip's own path: {e}"))?;
            if *print {
                Ok(hook::snippet(&exe))
            } else {
                let path = hook::default_settings_path();
                match hook::install(&path, &exe)? {
                    hook::Outcome::Installed { backup } => {
                        let b = match backup {
                            Some(p) => format!(" (backup: {})", p.display()),
                            None => " (new file)".to_string(),
                        };
                        Ok(format!(
                            "installed wip SessionStart hook in {}{}\n",
                            path.display(),
                            b
                        ))
                    }
                    hook::Outcome::AlreadyPresent => Ok(format!(
                        "wip hook already present in {} (no change)\n",
                        path.display()
                    )),
                }
            }
        }
        Some(cli::Command::InstallSkill) => {
            let home = skill::default_home()?;
            let written = skill::install(&home)?;
            let lines: String = written
                .iter()
                .map(|p| format!("  {}\n", p.display()))
                .collect();
            Ok(format!("installed wip skill:\n{lines}"))
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
