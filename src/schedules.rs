use crate::dates;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CACHE_MAX_AGE_SECONDS: i64 = 36 * 60 * 60;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScheduleCache {
    pub fetched_at: i64,
    #[serde(default)]
    pub workflows: Vec<WorkflowHealth>,
    #[serde(default)]
    pub errors: Vec<RepoFetchError>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowHealth {
    pub repo: String,
    pub workflow: String,
    pub latest_result: String,
    pub consecutive_failures: u32,
    pub last_success: Option<i64>,
    pub last_run: i64,
    pub median_interval_seconds: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoFetchError {
    pub repo: String,
    pub message: String,
}

pub trait GhApi {
    fn api(&self, repo_path: &Path, endpoint: &str) -> Result<String, String>;
}

pub struct CommandGh {
    pub binary: String,
}

impl GhApi for CommandGh {
    fn api(&self, repo_path: &Path, endpoint: &str) -> Result<String, String> {
        let output = Command::new(&self.binary)
            .current_dir(repo_path)
            .args(["api", endpoint])
            .output()
            .map_err(|e| format!("無法執行 gh：{e}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

pub fn default_cache_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".cache/wip/schedules.json")
}

pub fn read_cache(path: &Path) -> Result<Option<ScheduleCache>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path)
        .map_err(|e| format!("無法讀取排程快取 {}：{e}", path.display()))?;
    let cache = serde_json::from_str(&contents)
        .map_err(|e| format!("排程快取格式錯誤 {}：{e}", path.display()))?;
    Ok(Some(cache))
}

pub fn write_cache(path: &Path, cache: &ScheduleCache) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("無法建立排程快取目錄 {}：{e}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(cache).map_err(|e| e.to_string())? + "\n";
    fs::write(path, content).map_err(|e| format!("無法寫入排程快取 {}：{e}", path.display()))
}

pub fn refresh(repos: &[PathBuf], gh: &impl GhApi, fetched_at: i64) -> ScheduleCache {
    let mut cache = ScheduleCache {
        fetched_at,
        ..Default::default()
    };
    for repo_path in repos {
        let repo = repo_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| repo_path.display().to_string());
        let remote = match github_identity(repo_path) {
            Some(remote) => remote,
            None => {
                cache.errors.push(RepoFetchError {
                    repo,
                    message: "找不到 GitHub remote".into(),
                });
                continue;
            }
        };
        let endpoint = format!("repos/{remote}/actions/runs?per_page=100");
        match gh.api(repo_path, &endpoint) {
            Ok(body) => match parse_runs(&repo, &body) {
                Ok(mut workflows) => cache.workflows.append(&mut workflows),
                Err(message) => cache.errors.push(RepoFetchError { repo, message }),
            },
            Err(message) => cache.errors.push(RepoFetchError { repo, message }),
        }
    }
    cache
}

pub fn parse_runs(repo: &str, raw: &str) -> Result<Vec<WorkflowHealth>, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("GitHub 回傳 JSON 無法解析：{e}"))?;
    let runs = value
        .get("workflow_runs")
        .and_then(Value::as_array)
        .ok_or_else(|| "GitHub 回傳內容缺少 workflow_runs 陣列".to_string())?;
    // 依 workflow_id 分組（改名後仍算同一支），名稱取最新一次執行的。
    // 只收「出現過排程觸發」的 workflow；連敗與最後成功看全部觸發方式
    // （手動重跑修好了就算修好），間隔只看排程觸發。
    struct Run {
        ts: i64,
        result: String,
        scheduled: bool,
        name: String,
    }
    let mut grouped: BTreeMap<String, Vec<Run>> = BTreeMap::new();
    for run in runs {
        let name = run
            .get("workflow_name")
            .or_else(|| run.get("name"))
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        let key = run
            .get("workflow_id")
            .map(|id| format!("workflow-{}", id))
            .or_else(|| name.clone())
            .unwrap_or_else(|| "workflow-unknown".into());
        let timestamp = ["run_started_at", "created_at", "updated_at"]
            .iter()
            .filter_map(|k| run.get(*k).and_then(Value::as_str))
            .find_map(dates::parse_timestamp);
        let Some(ts) = timestamp else { continue };
        let result = run
            .get("conclusion")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| run.get("status").and_then(Value::as_str))
            .unwrap_or("unknown")
            .to_ascii_lowercase();
        // 沒有 event 欄位時視為排程（相容舊資料與測試）
        let scheduled = run
            .get("event")
            .and_then(Value::as_str)
            .map(|e| e == "schedule")
            .unwrap_or(true);
        grouped.entry(key.clone()).or_default().push(Run {
            ts,
            result,
            scheduled,
            name: name.unwrap_or(key),
        });
    }

    let mut output = Vec::new();
    for (_, mut runs) in grouped {
        if !runs.iter().any(|r| r.scheduled) {
            continue;
        }
        runs.sort_by(|a, b| b.ts.cmp(&a.ts));
        let workflow = runs[0].name.clone();
        let latest_result = runs[0].result.clone();
        let consecutive_failures = runs.iter().take_while(|r| is_failure(&r.result)).count() as u32;
        let last_success = runs.iter().find(|r| r.result == "success").map(|r| r.ts);
        let scheduled: Vec<i64> = runs.iter().filter(|r| r.scheduled).map(|r| r.ts).collect();
        let last_run = scheduled.first().copied().unwrap_or_default();
        output.push(WorkflowHealth {
            repo: repo.to_string(),
            workflow,
            latest_result,
            consecutive_failures,
            last_success,
            last_run,
            median_interval_seconds: median(burst_gaps(&scheduled)),
        });
    }
    Ok(output)
}

/// GitHub 排程常在同一段時間連跑好幾次（一天兩三批），
/// 把相隔不到 3 小時的執行併成一批，間隔只算批與批之間，
/// 否則中位數會被批內的幾分鐘拉低，正常排程被誤報成沒執行。
/// `desc` 為新到舊排序的時間戳。
fn burst_gaps(desc: &[i64]) -> Vec<i64> {
    const BURST_SECONDS: i64 = 3 * 3600;
    let mut heads: Vec<i64> = Vec::new(); // 每批最新的一次
    let mut prev: Option<i64> = None;
    for &ts in desc {
        if prev.map(|p| p - ts > BURST_SECONDS).unwrap_or(true) {
            heads.push(ts);
        }
        prev = Some(ts);
    }
    heads.windows(2).map(|w| w[0] - w[1]).collect()
}

fn is_failure(result: &str) -> bool {
    matches!(
        result,
        "failure" | "timed_out" | "startup_failure" | "action_required"
    )
}

fn median(mut values: Vec<i64>) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let middle = values.len() / 2;
    Some(if values.len() % 2 == 1 {
        values[middle]
    } else {
        (values[middle - 1] + values[middle]) / 2
    })
}

pub fn needs_attention(workflow: &WorkflowHealth, now: i64) -> bool {
    workflow.consecutive_failures >= 2
        || workflow
            .median_interval_seconds
            .map(|interval| {
                interval > 0 && now.saturating_sub(workflow.last_run) > interval.saturating_mul(2)
            })
            .unwrap_or(false)
}

pub fn cache_is_stale(cache: Option<&ScheduleCache>, now: i64) -> bool {
    cache
        .map(|cache| now.saturating_sub(cache.fetched_at) > CACHE_MAX_AGE_SECONDS)
        .unwrap_or(true)
}

pub fn table(cache: &ScheduleCache) -> String {
    let mut out = String::from("repo\tworkflow\t最近結果\t連敗\t最後成功\t中位間隔\n");
    for workflow in &cache.workflows {
        let last_success = workflow
            .last_success
            .map(dates::format_timestamp)
            .unwrap_or_else(|| "無".into());
        let interval = workflow
            .median_interval_seconds
            .map(format_interval)
            .unwrap_or_else(|| "無".into());
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            workflow.repo,
            workflow.workflow,
            workflow.latest_result,
            workflow.consecutive_failures,
            last_success,
            interval
        ));
    }
    for error in &cache.errors {
        out.push_str(&format!("{}\t擷取失敗\t{}\n", error.repo, error.message));
    }
    if cache.workflows.is_empty() && cache.errors.is_empty() {
        out.push_str("沒有排程 workflow 紀錄\n");
    }
    out
}

fn format_interval(seconds: i64) -> String {
    if seconds >= 86_400 {
        format!("{} 天", seconds / 86_400)
    } else if seconds >= 3600 {
        format!("{} 小時", seconds / 3600)
    } else {
        format!("{} 分鐘", seconds / 60)
    }
}

pub fn github_identity(repo_path: &Path) -> Option<String> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["remote"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let remotes = String::from_utf8_lossy(&output.stdout);
    remotes.lines().find_map(|name| {
        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["remote", "get-url", name])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_github_remote(String::from_utf8_lossy(&output.stdout).trim())
    })
}

pub fn parse_github_remote(remote: &str) -> Option<String> {
    let path = if let Some(rest) = remote.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = remote.strip_prefix("http://github.com/") {
        rest
    } else if let Some(rest) = remote.strip_prefix("ssh://git@github.com/") {
        rest
    } else if let Some(rest) = remote.strip_prefix("git@github.com:") {
        rest
    } else {
        return None;
    };
    let clean = path
        .trim_end_matches('/')
        .strip_suffix(".git")
        .unwrap_or(path.trim_end_matches('/'));
    let mut parts = clean.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const RUNS: &str = r#"{"workflow_runs":[
      {"name":"nightly-check","workflow_id":7,"conclusion":"failure","run_started_at":"2026-11-12T00:00:00Z"},
      {"name":"nightly-check","workflow_id":7,"conclusion":"failure","run_started_at":"2026-11-11T00:00:00Z"},
      {"name":"nightly-check","workflow_id":7,"conclusion":"success","run_started_at":"2026-11-10T00:00:00Z"},
      {"name":"release","workflow_id":9,"conclusion":"success","run_started_at":"2026-11-12T01:00:00Z"}
    ]}"#;

    #[test]
    fn groups_runs_and_counts_failure_streak_and_median() {
        let workflows = parse_runs("demo", RUNS).unwrap();
        let nightly = workflows
            .iter()
            .find(|w| w.workflow == "nightly-check")
            .unwrap();
        assert_eq!(nightly.consecutive_failures, 2);
        assert_eq!(nightly.latest_result, "failure");
        assert_eq!(nightly.median_interval_seconds, Some(86_400));
        assert!(nightly.last_success.is_some());
    }

    #[test]
    fn bursts_collapse_so_normal_schedule_is_not_flagged() {
        // 一天兩批、每批 3 次、批內相隔 30 分鐘
        let day = 86_400;
        let mut ts = vec![];
        for d in 0..3 {
            for b in [0, 13 * 3600] {
                for i in 0..3 {
                    ts.push(100 * day - d * day - b - i * 1800);
                }
            }
        }
        ts.sort_unstable_by(|a, b| b.cmp(a));
        let gaps = burst_gaps(&ts);
        assert!(median(gaps).unwrap() > 3 * 3600);
    }

    #[test]
    fn manual_rerun_success_ends_failure_streak_and_rename_stays_one_workflow() {
        let raw = r#"{"workflow_runs":[
          {"name":"新名","workflow_id":5,"event":"workflow_dispatch","conclusion":"success","run_started_at":"2026-11-12T00:00:00Z"},
          {"name":"舊名","workflow_id":5,"event":"schedule","conclusion":"failure","run_started_at":"2026-11-09T00:00:00Z"},
          {"name":"舊名","workflow_id":5,"event":"schedule","conclusion":"failure","run_started_at":"2026-11-02T00:00:00Z"},
          {"name":"tests","workflow_id":6,"event":"push","conclusion":"failure","run_started_at":"2026-11-12T00:00:00Z"}
        ]}"#;
        let w = parse_runs("demo", raw).unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].workflow, "新名");
        assert_eq!(w[0].consecutive_failures, 0);
    }

    #[test]
    fn reports_missed_expected_run_after_two_intervals() {
        let mut workflow = parse_runs("demo", RUNS).unwrap().remove(0);
        workflow.consecutive_failures = 0;
        workflow.last_run = 1_000;
        workflow.median_interval_seconds = Some(100);
        assert!(needs_attention(&workflow, 1_201));
        assert!(!needs_attention(&workflow, 1_200));
    }

    #[test]
    fn marks_missing_or_old_cache_as_stale_after_36_hours() {
        let cache = ScheduleCache {
            fetched_at: 1_000,
            ..Default::default()
        };
        assert!(cache_is_stale(None, 10_000));
        assert!(!cache_is_stale(Some(&cache), 1_000 + CACHE_MAX_AGE_SECONDS));
        assert!(cache_is_stale(Some(&cache), 1_001 + CACHE_MAX_AGE_SECONDS));
    }

    #[test]
    fn parses_common_github_remote_forms() {
        assert_eq!(
            parse_github_remote("git@github.com:yazelin/wip.git").as_deref(),
            Some("yazelin/wip")
        );
        assert_eq!(
            parse_github_remote("https://github.com/yazelin/wip").as_deref(),
            Some("yazelin/wip")
        );
        assert_eq!(parse_github_remote("https://example.com/yazelin/wip"), None);
    }

    struct FakeGh;

    impl GhApi for FakeGh {
        fn api(&self, repo_path: &Path, endpoint: &str) -> Result<String, String> {
            assert_eq!(
                endpoint,
                "repos/yazelin/wip/actions/runs?per_page=100"
            );
            if repo_path.file_name().and_then(|s| s.to_str()) == Some("bad") {
                Err("fake failure".into())
            } else {
                Ok(RUNS.into())
            }
        }
    }

    #[test]
    fn refresh_continues_after_one_repo_failure_and_records_error() {
        let temp = TempDir::new().unwrap();
        let ok = temp.path().join("ok");
        let bad = temp.path().join("bad");
        std::fs::create_dir_all(&ok).unwrap();
        std::fs::create_dir_all(&bad).unwrap();
        // 實際 remote 解析由 parser 測試覆蓋；這裡用 fake remote repo。
        init_repo_with_remote(&ok);
        init_repo_with_remote(&bad);
        let cache = refresh(&[ok, bad], &FakeGh, 5_000);
        assert_eq!(cache.workflows.len(), 2);
        assert_eq!(cache.errors.len(), 1);
        assert_eq!(cache.errors[0].message, "fake failure");
    }

    fn init_repo_with_remote(path: &Path) {
        let _ = Command::new("git")
            .current_dir(path)
            .args(["init", "-q"])
            .status();
        let _ = Command::new("git")
            .current_dir(path)
            .args(["remote", "add", "origin", "git@github.com:yazelin/wip.git"])
            .status();
    }

    #[test]
    fn reads_and_writes_cache() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("schedules.json");
        assert!(read_cache(&path).unwrap().is_none());
        write_cache(
            &path,
            &ScheduleCache {
                fetched_at: 123,
                workflows: parse_runs("demo", RUNS).unwrap(),
                errors: vec![],
            },
        )
        .unwrap();
        let cache = read_cache(&path).unwrap().unwrap();
        assert_eq!(cache.fetched_at, 123);
        assert_eq!(cache.workflows.len(), 2);
    }
}
