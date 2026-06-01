use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct LastCommit {
    pub rel_time: String,
    pub message: String,
    pub sha: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OpenPr {
    pub number: u64,
    pub title: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RepoStatus {
    pub name: String,
    pub path: String,
    pub branch: String,
    pub last_commit: Option<LastCommit>,
    pub dirty_files: u32,
    pub unpushed: u32,
    pub open_prs: Vec<OpenPr>,
    pub open_issues: Option<u32>,
    pub gh_available: bool,
    pub progress_tail: Option<String>,
    pub next_actions: Vec<String>,  // v2: open items from <repo>/NEXT.md
    pub planning_docs: Vec<String>, // v2: detected planning files (roadmap/TODO/...), pointers only
    pub commit_ts: i64,             // committer unix time, for sorting
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_status_default_is_empty() {
        let r = RepoStatus::default();
        assert_eq!(r.name, "");
        assert_eq!(r.dirty_files, 0);
        assert!(r.last_commit.is_none());
        assert!(r.next_actions.is_empty());
    }
}
