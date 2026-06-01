use crate::model::RepoStatus;

pub fn json(statuses: &[RepoStatus]) -> String {
    serde_json::to_string_pretty(statuses).unwrap_or_else(|_| "[]".into())
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
}
