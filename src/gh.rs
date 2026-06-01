use crate::model::OpenPr;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Deserialize)]
struct GhPr {
    number: u64,
    title: String,
}

#[derive(Deserialize)]
struct GhIssue {
    #[allow(dead_code)]
    number: u64,
}

pub struct GhInfo {
    pub available: bool,
    pub prs: Vec<OpenPr>,
    pub open_issues: Option<u32>,
}

impl GhInfo {
    /// The degraded result used when gh is unavailable or intentionally skipped.
    pub fn unavailable() -> GhInfo {
        GhInfo {
            available: false,
            prs: vec![],
            open_issues: None,
        }
    }
}

fn gh_stdout(repo: &Path, gh_bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(gh_bin)
        .current_dir(repo)
        .args(args)
        .output()
        .ok()?; // spawn error (gh not installed) -> None
    if !out.status.success() {
        return None; // not a gh repo / not authed -> None
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn collect(repo: &Path, gh_bin: &str) -> GhInfo {
    let prs_raw = gh_stdout(repo, gh_bin, &["pr", "list", "--json", "number,title"]);
    let issues_raw = gh_stdout(repo, gh_bin, &["issue", "list", "--json", "number"]);
    combine(prs_raw, issues_raw)
}

/// Combine the raw outputs of `gh pr list` / `gh issue list` into a `GhInfo`.
///
/// Pure (no IO) so the degradation logic is unit-testable without invoking
/// `gh`. A single failing query no longer discards the data we did get: the
/// repo is "available" when at least one query succeeded, and whichever field
/// failed is left empty (`prs == []`) or unknown (`open_issues == None`).
fn combine(prs_raw: Option<String>, issues_raw: Option<String>) -> GhInfo {
    if prs_raw.is_none() && issues_raw.is_none() {
        return GhInfo::unavailable();
    }
    let prs = prs_raw
        .and_then(|pj| serde_json::from_str::<Vec<GhPr>>(&pj).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|p| OpenPr {
            number: p.number,
            title: p.title,
        })
        .collect();
    let open_issues = issues_raw
        .and_then(|ij| serde_json::from_str::<Vec<GhIssue>>(&ij).ok())
        .map(|v| v.len() as u32);
    GhInfo {
        available: true,
        prs,
        open_issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn missing_gh_binary_degrades() {
        let info = collect(Path::new("/tmp"), "wip-no-such-gh-binary-xyz");
        assert!(!info.available);
        assert!(info.prs.is_empty());
        assert!(info.open_issues.is_none());
    }

    #[test]
    fn unavailable_constructor() {
        let g = GhInfo::unavailable();
        assert!(!g.available);
        assert!(g.prs.is_empty());
        assert!(g.open_issues.is_none());
    }

    #[test]
    fn combine_both_present() {
        let g = combine(
            Some(r#"[{"number":7,"title":"a"}]"#.to_string()),
            Some(r#"[{"number":3},{"number":4}]"#.to_string()),
        );
        assert!(g.available);
        assert_eq!(g.prs.len(), 1);
        assert_eq!(g.prs[0].number, 7);
        assert_eq!(g.open_issues, Some(2));
    }

    #[test]
    fn combine_only_prs_keeps_prs() {
        // issue query failed (e.g. Issues disabled) but PRs succeeded:
        // PR data must survive, issue count is unknown.
        let g = combine(
            Some(r#"[{"number":9,"title":"keep me"}]"#.to_string()),
            None,
        );
        assert!(g.available);
        assert_eq!(g.prs.len(), 1);
        assert_eq!(g.prs[0].number, 9);
        assert_eq!(g.open_issues, None);
    }

    #[test]
    fn combine_only_issues_keeps_count() {
        let g = combine(
            None,
            Some(r#"[{"number":1},{"number":2},{"number":5}]"#.to_string()),
        );
        assert!(g.available);
        assert!(g.prs.is_empty());
        assert_eq!(g.open_issues, Some(3));
    }

    #[test]
    fn combine_neither_degrades() {
        let g = combine(None, None);
        assert!(!g.available);
        assert!(g.prs.is_empty());
        assert_eq!(g.open_issues, None);
    }
}
