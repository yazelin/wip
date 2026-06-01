use std::path::{Path, PathBuf};

/// The skill definition, embedded at compile time so a release binary can
/// install it with no repo present (same spirit as `install-hook`).
pub const SKILL_MD: &str = include_str!("../skill/SKILL.md");

/// Where the skill is installed, given a home directory: one entry per agent
/// that consumes skills (Claude Code, Codex). Pure so it is unit-testable.
pub fn skill_targets(home: &Path) -> Vec<PathBuf> {
    [".claude/skills/wip", ".codex/skills/wip"]
        .iter()
        .map(|rel| home.join(rel).join("SKILL.md"))
        .collect()
}

/// Resolve the home directory the way the rest of wip does (`$HOME`).
pub fn default_home() -> Result<PathBuf, String> {
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() => Ok(PathBuf::from(h)),
        _ => Err("HOME is not set".to_string()),
    }
}

/// Write the embedded SKILL.md into every target under `home`, creating parent
/// dirs as needed. Idempotent: re-running overwrites with identical content.
/// Returns the paths written, in order.
pub fn install(home: &Path) -> Result<Vec<PathBuf>, String> {
    let targets = skill_targets(home);
    for path in &targets {
        let parent = path
            .parent()
            .ok_or_else(|| format!("{} has no parent dir", path.display()))?;
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        std::fs::write(path, SKILL_MD)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn targets_cover_claude_and_codex() {
        let home = Path::new("/home/x");
        let t = skill_targets(home);
        assert_eq!(t.len(), 2);
        assert!(t.contains(&PathBuf::from("/home/x/.claude/skills/wip/SKILL.md")));
        assert!(t.contains(&PathBuf::from("/home/x/.codex/skills/wip/SKILL.md")));
    }

    #[test]
    fn embedded_skill_has_frontmatter() {
        assert!(SKILL_MD.starts_with("---"));
        assert!(SKILL_MD.contains("name: wip"));
        assert!(SKILL_MD.contains("description:"));
    }

    #[test]
    fn install_writes_both_targets_with_embedded_content() {
        let d = TempDir::new().unwrap();
        let written = install(d.path()).unwrap();
        assert_eq!(written.len(), 2);
        for p in &written {
            assert!(p.exists(), "{} not written", p.display());
            assert_eq!(std::fs::read_to_string(p).unwrap(), SKILL_MD);
        }
    }

    #[test]
    fn install_creates_missing_parent_dirs() {
        let d = TempDir::new().unwrap();
        // home exists but the .claude / .codex trees do not yet.
        assert!(install(d.path()).is_ok());
        assert!(d.path().join(".claude/skills/wip/SKILL.md").exists());
        assert!(d.path().join(".codex/skills/wip/SKILL.md").exists());
    }

    #[test]
    fn install_is_idempotent() {
        let d = TempDir::new().unwrap();
        install(d.path()).unwrap();
        let first = std::fs::read_to_string(d.path().join(".claude/skills/wip/SKILL.md")).unwrap();
        install(d.path()).unwrap(); // second run must not error
        let second = std::fs::read_to_string(d.path().join(".claude/skills/wip/SKILL.md")).unwrap();
        assert_eq!(first, second);
    }
}
