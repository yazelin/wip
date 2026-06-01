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

    match (prs_raw, issues_raw) {
        (Some(pj), Some(ij)) => {
            let prs: Vec<GhPr> = serde_json::from_str(&pj).unwrap_or_default();
            let issues: Vec<GhIssue> = serde_json::from_str(&ij).unwrap_or_default();
            GhInfo {
                available: true,
                prs: prs
                    .into_iter()
                    .map(|p| OpenPr {
                        number: p.number,
                        title: p.title,
                    })
                    .collect(),
                open_issues: Some(issues.len() as u32),
            }
        }
        _ => GhInfo {
            available: false,
            prs: vec![],
            open_issues: None,
        },
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
}
