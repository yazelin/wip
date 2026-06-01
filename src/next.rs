use std::fs;
use std::path::{Path, PathBuf};

fn next_path(repo: &Path) -> PathBuf {
    repo.join("NEXT.md")
}

/// An open item is a line that, after trimming leading whitespace, starts with
/// "- [ ] ". Returns the text after that prefix.
fn parse_open(line: &str) -> Option<String> {
    line.trim_start().strip_prefix("- [ ] ").map(|s| s.to_string())
}

/// Open next-action texts from `<repo>/NEXT.md`, in file order.
/// Missing/unreadable file -> empty (not an error).
pub fn read_open(repo: &Path) -> Vec<String> {
    let content = match fs::read_to_string(next_path(repo)) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    content.lines().filter_map(parse_open).collect()
}

/// Append "- [ ] <text>" to `<repo>/NEXT.md`, creating the file (with a header)
/// if absent.
pub fn add(repo: &Path, text: &str) -> std::io::Result<()> {
    let path = next_path(repo);
    let mut content = fs::read_to_string(&path).unwrap_or_default();
    if content.is_empty() {
        content.push_str("# Next\n\n");
    } else if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("- [ ] {text}\n"));
    fs::write(&path, content)
}

/// Flip the n-th OPEN item (1-based) to "- [x] ", preserving leading whitespace
/// and text. Returns the completed item's text. Errors if n is out of range.
pub fn mark_done(repo: &Path, n: usize) -> Result<String, String> {
    let path = next_path(repo);
    let content =
        fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut open_seen = 0;
    let mut done_text = None;
    for line in lines.iter_mut() {
        if let Some(text) = parse_open(line) {
            open_seen += 1;
            if open_seen == n {
                let ws_len = line.len() - line.trim_start().len();
                let ws = line[..ws_len].to_string();
                *line = format!("{ws}- [x] {text}");
                done_text = Some(text);
                break;
            }
        }
    }
    match done_text {
        Some(t) => {
            let mut out = lines.join("\n");
            if content.ends_with('\n') {
                out.push('\n');
            }
            fs::write(&path, out).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            Ok(t)
        }
        None => Err(format!("no open item #{n} (only {open_seen} open)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_next(d: &Path, body: &str) {
        std::fs::write(d.join("NEXT.md"), body).unwrap();
    }

    #[test]
    fn read_open_returns_only_open_items_in_order() {
        let d = TempDir::new().unwrap();
        write_next(
            d.path(),
            "# Next\n\n- [ ] first\n- [x] done one\n- [ ] second\nsome prose\n",
        );
        assert_eq!(read_open(d.path()), vec!["first".to_string(), "second".to_string()]);
    }

    #[test]
    fn read_open_empty_when_no_file() {
        let d = TempDir::new().unwrap();
        assert!(read_open(d.path()).is_empty());
    }

    #[test]
    fn add_creates_file_with_header() {
        let d = TempDir::new().unwrap();
        add(d.path(), "do thing").unwrap();
        let c = std::fs::read_to_string(d.path().join("NEXT.md")).unwrap();
        assert!(c.starts_with("# Next\n"));
        assert!(c.contains("- [ ] do thing\n"));
    }

    #[test]
    fn add_appends_to_existing() {
        let d = TempDir::new().unwrap();
        write_next(d.path(), "# Next\n\n- [ ] one\n");
        add(d.path(), "two").unwrap();
        assert_eq!(read_open(d.path()), vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn mark_done_flips_nth_open() {
        let d = TempDir::new().unwrap();
        write_next(d.path(), "- [ ] a\n- [ ] b\n- [ ] c\n");
        let t = mark_done(d.path(), 2).unwrap();
        assert_eq!(t, "b");
        assert_eq!(read_open(d.path()), vec!["a".to_string(), "c".to_string()]);
        let c = std::fs::read_to_string(d.path().join("NEXT.md")).unwrap();
        assert!(c.contains("- [x] b"));
    }

    #[test]
    fn mark_done_out_of_range_errors() {
        let d = TempDir::new().unwrap();
        write_next(d.path(), "- [ ] only\n");
        let e = mark_done(d.path(), 5).unwrap_err();
        assert!(e.contains("no open item"));
    }
}
