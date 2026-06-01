use std::path::Path;

/// Common planning-file names (lowercase) we surface as pointers. Content is
/// never parsed - we only report which exist.
const PLANNING_NAMES: &[&str] = &["roadmap.md", "todo.md", "todo", "plan.md", "backlog.md"];

/// Names of planning files present in the repo root (case-insensitive match),
/// sorted for deterministic output. Empty if none / unreadable.
pub fn detect(repo: &Path) -> Vec<String> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(repo) {
        for e in entries.flatten() {
            if !e.path().is_file() {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if PLANNING_NAMES.contains(&name.to_lowercase().as_str()) {
                found.push(name);
            }
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detects_planning_files_case_insensitive() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("ROADMAP.md"), "x").unwrap();
        std::fs::write(d.path().join("TODO.md"), "x").unwrap();
        std::fs::write(d.path().join("README.md"), "x").unwrap(); // not a planning file
        assert_eq!(
            detect(d.path()),
            vec!["ROADMAP.md".to_string(), "TODO.md".to_string()]
        );
    }

    #[test]
    fn empty_when_none() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("README.md"), "x").unwrap();
        assert!(detect(d.path()).is_empty());
    }
}
