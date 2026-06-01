use crate::model::RepoStatus;

pub fn json(statuses: &[RepoStatus]) -> String {
    serde_json::to_string_pretty(statuses).unwrap_or_else(|_| "[]".into())
}

/// Markdown digest for AI agents to ingest. ASCII only, no emoji.
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
        s.push('\n');
    }
    s
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
        assert!(out.contains("gh unavailable")); // sample has gh_available=false
        // no emoji: assert the warn/check symbols are absent
        assert!(!out.contains('\u{26A0}')); // no WARNING SIGN
        assert!(!out.contains('\u{2713}')); // no CHECK MARK
    }
}
