use crate::dates;
use crate::model::NextAction;
use std::fs;
use std::path::{Path, PathBuf};

fn next_path(repo: &Path) -> PathBuf {
    repo.join("NEXT.md")
}

/// 移除行首空白後，以 `- [ ] ` 開頭的行視為未完成待辦，並回傳後續文字。
fn parse_open(line: &str) -> Option<String> {
    line.trim_start()
        .strip_prefix("- [ ] ")
        .map(|s| s.to_string())
}

/// 讀取行尾的其中一種到期日格式；格式錯誤時保留原文且不設到期日。
pub fn parse_action(text: &str) -> NextAction {
    for marker in ["（到期 ", " (due: ", "(due: "] {
        if let Some(start) = text.rfind(marker) {
            let suffix = &text[start..];
            let date = if marker == "（到期 " {
                suffix
                    .strip_prefix("（到期 ")
                    .and_then(|s| s.strip_suffix('）'))
            } else {
                suffix
                    .strip_prefix(marker)
                    .and_then(|s| s.strip_suffix(')'))
            };
            if let Some(date) = date {
                if dates::parse_date(date).is_some() {
                    return NextAction {
                        text: text[..start].trim_end().to_string(),
                        due: Some(date.to_string()),
                    };
                }
            }
        }
    }
    NextAction {
        text: text.to_string(),
        due: None,
    }
}

/// 讀取尚未完成的待辦明細，包含有效的到期日。
pub fn read_open_items(repo: &Path) -> Vec<NextAction> {
    let content = match fs::read_to_string(next_path(repo)) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    content
        .lines()
        .filter_map(parse_open)
        .map(|s| parse_action(&s))
        .collect()
}

/// 依檔案順序讀取 `<repo>/NEXT.md` 的未完成待辦。
/// 檔案不存在或無法讀取時回傳空清單。
pub fn read_open(repo: &Path) -> Vec<String> {
    let content = match fs::read_to_string(next_path(repo)) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    content.lines().filter_map(parse_open).collect()
}

/// 將待辦附加至 `<repo>/NEXT.md`；檔案不存在時建立檔案與標題。
pub fn add(repo: &Path, text: &str) -> std::io::Result<()> {
    add_with_due(repo, text, None)
}

/// 新增一項待辦，可選擇附上已驗證的到期日。
pub fn add_with_due(repo: &Path, text: &str, due: Option<&str>) -> std::io::Result<()> {
    let path = next_path(repo);
    let mut content = fs::read_to_string(&path).unwrap_or_default();
    if content.is_empty() {
        content.push_str("# Next\n\n");
    } else if !content.ends_with('\n') {
        content.push('\n');
    }
    match due {
        Some(date) => content.push_str(&format!("- [ ] {text} (due: {date})\n")),
        None => content.push_str(&format!("- [ ] {text}\n")),
    }
    fs::write(&path, content)
}

/// 將第 n 項未完成待辦改為 `- [x] `，保留行首空白與文字，並回傳完成的待辦。
/// 序號超出範圍時回傳錯誤。
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
        assert_eq!(
            read_open(d.path()),
            vec!["first".to_string(), "second".to_string()]
        );
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
        assert_eq!(
            read_open(d.path()),
            vec!["one".to_string(), "two".to_string()]
        );
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

    #[test]
    fn parses_both_due_suffix_formats() {
        assert_eq!(
            parse_action("ship release （到期 2026-11-20）"),
            NextAction {
                text: "ship release".into(),
                due: Some("2026-11-20".into())
            }
        );
        assert_eq!(
            parse_action("ship release (due: 2026-11-20)"),
            NextAction {
                text: "ship release".into(),
                due: Some("2026-11-20".into())
            }
        );
    }

    #[test]
    fn malformed_due_is_plain_text_without_error() {
        let item = parse_action("ship release (due: 2026-02-30)");
        assert_eq!(item.due, None);
        assert_eq!(item.text, "ship release (due: 2026-02-30)");
    }

    #[test]
    fn add_with_due_uses_ascii_convention() {
        let d = TempDir::new().unwrap();
        add_with_due(d.path(), "ship release", Some("2026-11-20")).unwrap();
        let content = std::fs::read_to_string(d.path().join("NEXT.md")).unwrap();
        assert!(content.contains("- [ ] ship release (due: 2026-11-20)"));
        assert_eq!(
            read_open_items(d.path())[0].due.as_deref(),
            Some("2026-11-20")
        );
    }
}
