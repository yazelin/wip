mod cli;
mod collector;
mod config;
mod dates;
mod gh;
mod git;
mod hook;
mod model;
mod next;
mod planning;
mod progress;
mod render;
mod schedules;
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

/// 將 `<repo>` 解析成路徑；若不是現有目錄，就依設定清單中的目錄名稱比對。
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

/// 排程健康快取超過這個秒數，就在開場時順手在背景更新一次。
const HOOK_BACKGROUND_REFRESH_AGE_SECONDS: i64 = 6 * 60 * 60;

/// 快取太舊（或還沒有）時，另起一個不等待的行程去更新，下一次開場就會讀到新的。
/// 開場摘要本身不打網路，也不等這個行程；起不來就靜默略過。
fn refresh_schedules_in_background(cache: Option<&schedules::ScheduleCache>, now: i64) {
    let fresh = cache
        .map(|c| now.saturating_sub(c.fetched_at) <= HOOK_BACKGROUND_REFRESH_AGE_SECONDS)
        .unwrap_or(false);
    if fresh || std::env::var_os("WIP_NO_BACKGROUND_REFRESH").is_some() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .args(["schedules", "--refresh"])
            .env("WIP_NO_BACKGROUND_REFRESH", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// 產生 SessionStart hook 的摘要；缺少設定或 repo 時保持靜默。
fn hook_context(args: &cli::Cli) -> String {
    match collect_sorted(args, false) {
        Ok(statuses) if !statuses.is_empty() => {
            let cache = schedules::read_cache(&schedules::default_cache_path())
                .ok()
                .flatten();
            let now = dates::now_unix();
            refresh_schedules_in_background(cache.as_ref(), now);
            render::hook_digest(&statuses, cache.as_ref(), now)
        }
        _ => String::new(),
    }
}

/// 產生一行 `install-hook` 安裝結果。
fn install_line(label: &str, path: &std::path::Path, outcome: hook::Outcome) -> String {
    match outcome {
        hook::Outcome::Installed { backup } => {
            let b = match backup {
                Some(p) => format!(" (backup: {})", p.display()),
                None => " (new file)".to_string(),
            };
            format!("installed wip hook ({label}) in {}{}\n", path.display(), b)
        }
        hook::Outcome::AlreadyPresent => {
            format!(
                "wip hook ({label}) already present in {} (no change)\n",
                path.display()
            )
        }
    }
}

fn run() -> Result<String, String> {
    let args = cli::Cli::parse();
    match &args.command {
        Some(cli::Command::Next { repo, text, due }) => {
            if let Some(date) = due {
                if dates::parse_date(date).is_none() {
                    return Err(format!("到期日格式錯誤：{date}，請用 YYYY-MM-DD"));
                }
            }
            let path = resolve_repo(repo, &args)?;
            match due.as_deref() {
                Some(date) => next::add_with_due(&path, text, Some(date)),
                None => next::add(&path, text),
            }
            .map_err(|e| format!("無法寫入 NEXT.md：{e}"))?;
            Ok(format!("已新增至 {}/NEXT.md：{text}\n", path.display()))
        }
        Some(cli::Command::Done { repo, n }) => {
            let path = resolve_repo(repo, &args)?;
            let done = next::mark_done(&path, *n)?;
            Ok(format!("done in {}: {done}\n", path.display()))
        }
        Some(cli::Command::Hook) => Ok(hook_context(&args)),
        Some(cli::Command::Schedules { refresh }) => {
            let cache_path = schedules::default_cache_path();
            if *refresh {
                let repos = resolve_repos(&args)?;
                let gh = schedules::CommandGh {
                    binary: args.gh_bin.clone(),
                };
                let cache = schedules::refresh(&repos, &gh, dates::now_unix());
                schedules::write_cache(&cache_path, &cache)?;
                Ok(schedules::table(&cache))
            } else {
                let cache = schedules::read_cache(&cache_path)?;
                match cache {
                    Some(cache) => Ok(schedules::table(&cache)),
                    None => Ok("尚無排程健康快取，請執行 wip schedules --refresh\n".into()),
                }
            }
        }
        Some(cli::Command::InstallHook { print }) => {
            let exe =
                std::env::current_exe().map_err(|e| format!("cannot find wip's own path: {e}"))?;
            if *print {
                Ok(hook::snippet(&exe))
            } else {
                let claude_path = hook::default_settings_path();
                let codex_path = hook::codex_hooks_path();
                let mut out = String::new();
                out.push_str(&install_line(
                    "Claude",
                    &claude_path,
                    hook::install(&claude_path, &exe)?,
                ));
                out.push_str(&install_line(
                    "Codex",
                    &codex_path,
                    hook::install_codex(&codex_path, &exe)?,
                ));
                out.push_str(&hook::codex_trust_notice(&codex_path));
                out.push('\n');
                Ok(out)
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
