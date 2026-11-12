use crate::dates;
use crate::model::RepoStatus;
use crate::schedules::{self, ScheduleCache};

pub fn json(statuses: &[RepoStatus]) -> String {
    serde_json::to_string_pretty(statuses).unwrap_or_else(|_| "[]".into())
}

/// 產生給 AI agent 讀取的 Markdown 摘要。
pub fn markdown(statuses: &[RepoStatus]) -> String {
    let mut s = String::from("# wip - dev status\n\n");
    for r in statuses {
        s.push_str(&format!("## {} ({})\n", r.name, r.branch));
        if let Some(e) = &r.error {
            s.push_str(&format!("- error: {e}\n\n"));
            continue;
        }
        if let Some(c) = &r.last_commit {
            s.push_str(&format!(
                "- last: {} - {} ({})\n",
                c.rel_time, c.message, c.sha
            ));
        }
        let mut flags = Vec::new();
        if r.dirty_files > 0 {
            flags.push(format!("{} dirty", r.dirty_files));
        }
        if r.unpushed > 0 {
            flags.push(format!("{} unpushed", r.unpushed));
        }
        if !flags.is_empty() {
            s.push_str(&format!("- local: {}\n", flags.join(", ")));
        }
        if r.gh_available {
            if r.open_prs.is_empty() {
                s.push_str("- PRs: none\n");
            } else {
                for pr in &r.open_prs {
                    s.push_str(&format!("- PR #{}: {}\n", pr.number, pr.title));
                }
            }
            if let Some(i) = r.open_issues {
                s.push_str(&format!("- open issues: {i}\n"));
            }
        } else {
            s.push_str("- PRs: - (gh unavailable)\n");
        }
        if let Some(p) = &r.progress_tail {
            let first = p.lines().next().unwrap_or("");
            s.push_str(&format!("- progress: {first}\n"));
        }
        if !r.next_actions.is_empty() {
            s.push_str("- next:\n");
            for (i, t) in r.next_actions.iter().enumerate() {
                let text = r
                    .next_action_details
                    .get(i)
                    .map(|item| item.text.as_str())
                    .unwrap_or(t);
                let due = r
                    .next_action_details
                    .get(i)
                    .and_then(|item| item.due.as_deref());
                let due = due
                    .map(|date| format!(" （到期 {date}）"))
                    .unwrap_or_default();
                s.push_str(&format!("  {}. {}{}\n", i + 1, text, due));
            }
        }
        if !r.planning_docs.is_empty() {
            s.push_str(&format!("- see: {}\n", r.planning_docs.join(", ")));
        }
        s.push('\n');
    }
    s
}

/// 產生給使用者查看的終端機摘要。
pub fn term(statuses: &[RepoStatus]) -> String {
    let mut s = format!("wip - {} repos (recent first)\n\n", statuses.len());
    for r in statuses {
        if let Some(e) = &r.error {
            s.push_str(&format!("{}  [error: {}]\n\n", r.name, e));
            continue;
        }
        s.push_str(&format!("{}  {}\n", r.name, r.branch));
        if let Some(c) = &r.last_commit {
            let mut line = format!("  last {}  \"{}\"", c.rel_time, c.message);
            let mut flags = Vec::new();
            if r.dirty_files > 0 {
                flags.push(format!("{} dirty", r.dirty_files));
            }
            if r.unpushed > 0 {
                flags.push(format!("{} unpushed", r.unpushed));
            }
            if !flags.is_empty() {
                line.push_str(&format!("  [{}]", flags.join(", ")));
            }
            s.push_str(&line);
            s.push('\n');
        }
        let pr_part = if !r.gh_available {
            "PR: -".to_string()
        } else if r.open_prs.is_empty() {
            "PR: none".to_string()
        } else {
            let list: Vec<String> = r
                .open_prs
                .iter()
                .map(|p| format!("#{}", p.number))
                .collect();
            format!("PR: {}", list.join(" "))
        };
        let issue_part = match r.open_issues {
            Some(i) => format!("   issues: {i}"),
            None => String::new(),
        };
        let progress_part = match &r.progress_tail {
            Some(p) => format!("   progress: {}", p.lines().next().unwrap_or("")),
            None => String::new(),
        };
        s.push_str(&format!("  {pr_part}{issue_part}{progress_part}\n"));
        if !r.next_actions.is_empty() {
            let items: Vec<String> = r
                .next_actions
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let detail = r.next_action_details.get(i);
                    let text = detail.map(|item| item.text.as_str()).unwrap_or(t);
                    let due = detail
                        .and_then(|item| item.due.as_deref())
                        .map(|date| format!(" （到期 {date}）"))
                        .unwrap_or_default();
                    format!("{}. {}{}", i + 1, text, due)
                })
                .collect();
            s.push_str(&format!("  next: {}\n", items.join("   ")));
        }
        if !r.planning_docs.is_empty() {
            s.push_str(&format!("  see: {}\n", r.planning_docs.join(", ")));
        }
        s.push('\n');
    }
    s
}

const HOOK_OUTPUT_MAX_BYTES: usize = 1800;
const HOOK_CACHE_MAX_AGE_NOTICE: &str = "排程健康資料已過期，跑 wip schedules --refresh";

/// 產生開場注入用的精簡摘要，並以 UTF-8 位元組數限制輸出長度。
pub fn hook_digest(statuses: &[RepoStatus], cache: Option<&ScheduleCache>, now: i64) -> String {
    if statuses.is_empty() {
        return String::new();
    }

    let today = dates::epoch_day_from_unix(now);
    let mut due_items = Vec::new();
    let mut schedule_items = Vec::new();
    let mut unpushed_items = Vec::new();
    let mut recent_items = Vec::new();
    let mut recent_count = 0usize;

    for status in statuses {
        let repo = clamp_bytes(&clean_inline(&status.name), 80);
        for action in &status.next_action_details {
            let Some(due) = action.due.as_deref() else {
                continue;
            };
            let Some(day) = dates::parse_date(due) else {
                continue;
            };
            let delta = day - today;
            if delta > 3 {
                continue;
            }
            let due_label = if delta < 0 {
                format!("（到期 {}，已過 {} 天）", &due[5..], -delta)
            } else if delta > 0 {
                format!("（到期 {}，還有 {} 天）", &due[5..], delta)
            } else {
                format!("（到期 {}）", &due[5..])
            };
            due_items.push((
                day,
                format!(
                    "{repo}：{}{}",
                    clamp_bytes(&clean_inline(&action.text), 300),
                    due_label
                ),
            ));
        }
        if let Some(c) = cache {
            for workflow in c
                .workflows
                .iter()
                .filter(|w| w.repo == status.name && schedules::needs_attention(w, now))
            {
                let last_success = workflow
                    .last_success
                    .map(|ts| {
                        dates::format_timestamp(ts)
                            .split(' ')
                            .next()
                            .unwrap_or("")
                            .to_string()
                    })
                    .unwrap_or_else(|| "無".into());
                let mut detail = if workflow.consecutive_failures >= 2 {
                    format!("連敗 {} 次", workflow.consecutive_failures)
                } else {
                    format!(
                        "超過預期間隔兩倍未執行，最近結果 {}",
                        workflow.latest_result
                    )
                };
                if workflow.consecutive_failures >= 2
                    && workflow
                        .median_interval_seconds
                        .map(|interval| {
                            interval > 0
                                && now.saturating_sub(workflow.last_run)
                                    > interval.saturating_mul(2)
                        })
                        .unwrap_or(false)
                {
                    detail.push_str("，超過預期間隔兩倍未執行");
                }
                schedule_items.push(format!(
                    "{}／{}：{}，最後成功 {}",
                    repo,
                    clamp_bytes(&clean_inline(&workflow.workflow), 100),
                    detail,
                    last_success
                ));
            }
        }
        if status.unpushed > 0 {
            unpushed_items.push(format!("{repo}：有 {} 個未推送提交", status.unpushed));
        }

        if status.commit_ts > 0 && now.saturating_sub(status.commit_ts) <= 7 * 86_400 {
            recent_count += 1;
            let title = status
                .last_commit
                .as_ref()
                .map(|commit| truncate_chars(&clean_inline(&commit.message), 40))
                .unwrap_or_default();
            let age = status
                .last_commit
                .as_ref()
                .map(|commit| commit.rel_time.as_str())
                .unwrap_or("剛剛");
            recent_items.push(format!(
                "{}：{}，{}，未提交 {} 檔，待辦 {} 項",
                repo,
                age,
                title,
                status.dirty_files,
                status.next_actions.len()
            ));
        }
    }

    due_items.sort_by_key(|item| item.0);
    let mut actions: Vec<String> = due_items.into_iter().map(|item| item.1).collect();
    actions.extend(schedule_items);
    actions.extend(unpushed_items);
    if schedules::cache_is_stale(cache, now) {
        actions.push(HOOK_CACHE_MAX_AGE_NOTICE.into());
    }

    let summary = format!(
        "其餘 {} 個 repo 沒有變化。",
        statuses.len().saturating_sub(recent_count)
    );
    let total_items = actions.len() + recent_items.len();
    let mut shown_actions = actions.len();
    let mut shown_recent = recent_items.len();
    loop {
        let omitted = total_items.saturating_sub(shown_actions + shown_recent);
        let output = format_hook_parts(
            &actions[..shown_actions],
            &recent_items[..shown_recent],
            &summary,
            omitted,
        );
        if output.len() <= HOOK_OUTPUT_MAX_BYTES {
            return output;
        }
        if shown_recent > 0 {
            shown_recent -= 1;
        } else if shown_actions > 0 {
            shown_actions -= 1;
        } else {
            return clamp_bytes(&summary, HOOK_OUTPUT_MAX_BYTES);
        }
    }
}

fn format_hook_parts(
    actions: &[String],
    recent: &[String],
    summary: &str,
    omitted: usize,
) -> String {
    let mut output = String::new();
    if !actions.is_empty() {
        output.push_str("要你看的\n");
        for item in actions {
            output.push_str("- ");
            output.push_str(item);
            output.push('\n');
        }
    }
    if !recent.is_empty() {
        output.push_str("最近在動的\n");
        for item in recent {
            output.push_str("- ");
            output.push_str(item);
            output.push('\n');
        }
    }
    output.push_str(summary);
    output.push('\n');
    if omitted > 0 {
        output.push_str(&format!("另有 {omitted} 項，wip --md 看全部\n"));
    }
    output
}

fn clean_inline(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clamp_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let suffix = "...";
    let mut end = max_bytes.saturating_sub(suffix.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{suffix}", &value[..end])
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let first: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        let prefix: String = first.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{prefix}...")
    } else {
        first
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LastCommit, RepoStatus};

    fn sample() -> RepoStatus {
        RepoStatus {
            name: "demo".into(),
            path: "/x/demo".into(),
            branch: "main".into(),
            last_commit: Some(LastCommit {
                rel_time: "2 hours ago".into(),
                message: "did thing".into(),
                sha: "abc123".into(),
            }),
            dirty_files: 2,
            commit_ts: 1000,
            ..Default::default()
        }
    }

    #[test]
    fn json_roundtrips_fields() {
        let out = json(&[sample()]);
        assert!(out.contains("\"name\": \"demo\""));
        assert!(out.contains("\"branch\": \"main\""));
        assert!(out.contains("did thing"));
        // valid json array
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v.is_array());
    }

    #[test]
    fn markdown_includes_repo_and_commit() {
        let out = markdown(&[sample()]);
        assert!(out.contains("## demo (main)"));
        assert!(out.contains("did thing"));
        assert!(out.contains("2 dirty"));
        // 樣本沒有 gh 資料，所以保留既有提示。
        assert!(out.contains("gh unavailable"));
        // 輸出不包含警告或勾選符號。
        assert!(!out.contains('\u{26A0}'));
        assert!(!out.contains('\u{2713}'));
    }

    #[test]
    fn term_renders_name_and_status() {
        let out = term(&[sample()]);
        assert!(out.contains("demo  main"));
        assert!(out.contains("\"did thing\""));
        assert!(out.contains("2 dirty"));
        assert!(out.contains("PR: -"));
        assert!(!out.contains('\u{26A0}'));
    }

    fn sample_with_extras() -> RepoStatus {
        let mut r = sample();
        r.next_actions = vec!["do A".into(), "do B".into()];
        r.planning_docs = vec!["ROADMAP.md".into()];
        r
    }

    #[test]
    fn term_renders_next_actions_numbered() {
        let out = term(&[sample_with_extras()]);
        assert!(out.contains("next: 1. do A   2. do B"));
    }

    #[test]
    fn term_renders_see_line() {
        let out = term(&[sample_with_extras()]);
        assert!(out.contains("see: ROADMAP.md"));
    }

    #[test]
    fn term_omits_next_and_see_when_empty() {
        let out = term(&[sample()]);
        assert!(!out.contains("next:"));
        assert!(!out.contains("see:"));
    }

    #[test]
    fn markdown_renders_next_actions_numbered() {
        let out = markdown(&[sample_with_extras()]);
        assert!(out.contains("- next:"));
        assert!(out.contains("1. do A"));
        assert!(out.contains("2. do B"));
    }

    #[test]
    fn markdown_renders_see_line() {
        let out = markdown(&[sample_with_extras()]);
        assert!(out.contains("- see: ROADMAP.md"));
    }

    #[test]
    fn markdown_omits_see_when_empty() {
        let out = markdown(&[sample()]);
        assert!(!out.contains("- see:"));
    }

    #[test]
    fn markdown_and_table_annotate_due_dates() {
        let mut r = sample();
        r.next_actions = vec!["ship release (due: 2026-11-20)".into()];
        r.next_action_details = vec![crate::model::NextAction {
            text: "ship release".into(),
            due: Some("2026-11-20".into()),
        }];
        let md = markdown(&[r.clone()]);
        let terminal = term(&[r]);
        assert!(md.contains("ship release （到期 2026-11-20）"));
        assert!(terminal.contains("ship release （到期 2026-11-20）"));
    }

    #[test]
    fn json_adds_due_details_without_changing_legacy_action_strings() {
        let mut r = sample();
        r.next_actions = vec!["ship release (due: 2026-11-20)".into()];
        r.next_action_details = vec![crate::model::NextAction {
            text: "ship release".into(),
            due: Some("2026-11-20".into()),
        }];
        let value: serde_json::Value = serde_json::from_str(&json(&[r])).unwrap();
        assert_eq!(
            value[0]["next_actions"][0],
            "ship release (due: 2026-11-20)"
        );
        assert_eq!(value[0]["next_action_details"][0]["due"], "2026-11-20");
    }

    #[test]
    fn json_uses_null_for_an_action_without_a_valid_due_date() {
        let mut r = sample();
        r.next_actions = vec!["ordinary task".into()];
        r.next_action_details = vec![crate::model::NextAction {
            text: "ordinary task".into(),
            due: None,
        }];
        let value: serde_json::Value = serde_json::from_str(&json(&[r])).unwrap();
        assert!(value[0]["next_action_details"][0]["due"].is_null());
    }

    #[test]
    fn hook_reads_fake_schedule_cache_and_emits_compact_digest() {
        let now = crate::dates::parse_date("2026-11-12").unwrap() * 86_400;
        let temp = tempfile::TempDir::new().unwrap();
        let cache_path = temp.path().join("schedules.json");
        let fake_json = format!(
            r#"{{"fetched_at":{now},"workflows":[{{"repo":"demo","workflow":"nightly","latest_result":"failure","consecutive_failures":2,"last_success":null,"last_run":{},"median_interval_seconds":86400}}],"errors":[]}}"#,
            now - 86_400
        );
        std::fs::write(&cache_path, fake_json).unwrap();
        let cache = schedules::read_cache(&cache_path).unwrap().unwrap();
        let mut status = sample();
        status.next_actions = vec!["prepare launch （到期 2026-11-13）".into()];
        status.next_action_details = vec![crate::model::NextAction {
            text: "prepare launch".into(),
            due: Some("2026-11-13".into()),
        }];
        status.unpushed = 2;
        status.commit_ts = now - 3600;
        let out = hook_digest(&[status], Some(&cache), now);
        assert!(out.contains("要你看的"));
        assert!(out.contains("demo：prepare launch（到期 11-13，還有 1 天）"));
        assert!(out.contains("demo／nightly：連敗 2 次，最後成功 無"));
        assert!(out.contains("demo：有 2 個未推送提交"));
        assert!(out.contains("最近在動的"));
        assert!(!out.contains("gh unavailable"));
    }

    #[test]
    fn hook_output_stays_under_byte_limit_and_counts_omitted_items() {
        let now = crate::dates::now_unix();
        let mut statuses = Vec::new();
        for index in 0..38 {
            let mut status = sample();
            status.name = format!("repo-{index}");
            status.commit_ts = now - 3600;
            status.last_commit.as_mut().unwrap().message = "長標題".repeat(40);
            status.next_actions = vec!["task".into(); 2];
            statuses.push(status);
        }
        let out = hook_digest(&statuses, None, now);
        assert!(out.len() <= HOOK_OUTPUT_MAX_BYTES);
        assert!(out.contains("另有 "));
    }
}
