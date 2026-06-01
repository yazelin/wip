use std::fs;
use std::path::Path;

const CANDIDATES: &[&str] = &["progress.md", "PROGRESS.md", "docs/progress.md"];
const CAP: usize = 400;

pub fn tail(repo: &Path) -> Option<String> {
    for c in CANDIDATES {
        let p = repo.join(c);
        if p.is_file() {
            let content = fs::read_to_string(&p).ok()?;
            return Some(last_section(&content));
        }
    }
    None
}

/// Return the last `## ` section of a markdown doc, trimmed and capped.
/// If there are no `## ` headings, return the whole (trimmed, capped) text.
fn last_section(content: &str) -> String {
    let mut sections: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in content.lines() {
        if line.starts_with("## ") && !cur.trim().is_empty() {
            sections.push(std::mem::take(&mut cur));
        }
        cur.push_str(line);
        cur.push('\n');
    }
    if !cur.trim().is_empty() {
        sections.push(cur);
    }
    let last = sections.last().map(|s| s.trim()).unwrap_or("");
    if last.chars().count() > CAP {
        let truncated: String = last.chars().take(CAP).collect();
        format!("{truncated}...")
    } else {
        last.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn last_section_returns_final_heading_block() {
        let md = "## Old\nold stuff\n\n## New\nnew stuff here\n";
        let out = last_section(md);
        assert!(out.starts_with("## New"));
        assert!(out.contains("new stuff here"));
        assert!(!out.contains("old stuff"));
    }

    #[test]
    fn no_headings_returns_whole_text() {
        let md = "just a note\nsecond line\n";
        let out = last_section(md);
        assert!(out.contains("just a note"));
        assert!(out.contains("second line"));
    }

    #[test]
    fn tail_reads_progress_file() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("progress.md"), "## Done\nshipped it\n").unwrap();
        let out = tail(d.path()).expect("tail");
        assert!(out.contains("shipped it"));
    }

    #[test]
    fn tail_none_when_absent() {
        let d = TempDir::new().unwrap();
        assert!(tail(d.path()).is_none());
    }
}
