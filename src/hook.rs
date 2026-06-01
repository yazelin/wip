use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Result of an install attempt.
pub enum Outcome {
    Installed { backup: Option<PathBuf> },
    AlreadyPresent,
}

/// Path to Claude Code's user settings: ~/.claude/settings.json
pub fn default_settings_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".claude/settings.json")
}

/// The command string the hook runs: the absolute wip binary + ` hook`.
pub fn hook_command(exe: &Path) -> String {
    format!("\"{}\" hook", exe.display())
}

/// A SessionStart entry (matcher "startup") that runs `wip hook`.
fn hook_entry(exe: &Path) -> Value {
    json!({
        "matcher": "startup",
        "hooks": [{ "type": "command", "command": hook_command(exe) }]
    })
}

/// Human-pasteable snippet for `--print`.
pub fn snippet(exe: &Path) -> String {
    format!(
        "Add this entry to hooks.SessionStart in {}:\n\n{}\n",
        default_settings_path().display(),
        serde_json::to_string_pretty(&hook_entry(exe)).unwrap()
    )
}

/// Does this command string look like a wip hook? Exact match against our
/// command, or any command ending in ` hook` that mentions wip (covers a
/// moved binary / different absolute path).
fn is_wip_hook_cmd(cmd: &str, exe: &Path) -> bool {
    if cmd == hook_command(exe) {
        return true;
    }
    // Fallback for a moved binary / different path: the command must invoke a
    // binary whose basename is `wip` (quoted or bare) with the `hook`
    // subcommand. Requiring the leading `/` avoids matching e.g. `swipe`.
    let trimmed = cmd.trim_end();
    trimmed.ends_with("/wip\" hook") || trimmed.ends_with("/wip hook") || trimmed == "wip hook"
}

fn already_installed(root: &Value, exe: &Path) -> bool {
    let entries = root
        .get("hooks")
        .and_then(|h| h.get("SessionStart"))
        .and_then(|v| v.as_array());
    let entries = match entries {
        Some(e) => e,
        None => return false,
    };
    for entry in entries {
        if let Some(hooks) = entry.get("hooks").and_then(|v| v.as_array()) {
            for h in hooks {
                if let Some(cmd) = h.get("command").and_then(|v| v.as_str()) {
                    if is_wip_hook_cmd(cmd, exe) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Idempotently add a SessionStart hook to `settings_path`. Backs the file up
/// (`.json.bak`) before modifying. Preserves all other keys and hooks.
pub fn install(settings_path: &Path, exe: &Path) -> Result<Outcome, String> {
    let existed = settings_path.exists();
    let mut root: Value = if existed {
        let data = std::fs::read_to_string(settings_path)
            .map_err(|e| format!("cannot read {}: {e}", settings_path.display()))?;
        if data.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&data)
                .map_err(|e| format!("malformed JSON in {}: {e}", settings_path.display()))?
        }
    } else {
        json!({})
    };

    if !root.is_object() {
        return Err(format!(
            "{} root is not a JSON object",
            settings_path.display()
        ));
    }

    if already_installed(&root, exe) {
        return Ok(Outcome::AlreadyPresent);
    }

    let backup = if existed {
        let bak = settings_path.with_extension("json.bak");
        std::fs::copy(settings_path, &bak)
            .map_err(|e| format!("cannot write backup {}: {e}", bak.display()))?;
        Some(bak)
    } else {
        None
    };

    let obj = root.as_object_mut().unwrap();
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| "hooks is not an object".to_string())?;
    let ss = hooks_obj
        .entry("SessionStart")
        .or_insert_with(|| json!([]));
    let arr = ss
        .as_array_mut()
        .ok_or_else(|| "hooks.SessionStart is not an array".to_string())?;
    arr.push(hook_entry(exe));

    let out = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? + "\n";
    std::fs::write(settings_path, out)
        .map_err(|e| format!("cannot write {}: {e}", settings_path.display()))?;
    Ok(Outcome::Installed { backup })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn exe() -> PathBuf {
        PathBuf::from("/usr/local/bin/wip")
    }

    #[test]
    fn install_creates_file_when_absent() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("settings.json");
        let outcome = install(&p, &exe()).unwrap();
        assert!(matches!(outcome, Outcome::Installed { backup: None }));
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let arr = v["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["matcher"], "startup");
        assert!(arr[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with("\" hook"));
    }

    #[test]
    fn install_preserves_existing_and_backs_up() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("settings.json");
        std::fs::write(
            &p,
            r#"{"permissions":{"x":1},"hooks":{"SessionStart":[{"matcher":"","hooks":[{"type":"command","command":"other-tool"}]}]}}"#,
        )
        .unwrap();
        let outcome = install(&p, &exe()).unwrap();
        assert!(matches!(outcome, Outcome::Installed { backup: Some(_) }));

        let bak = p.with_extension("json.bak");
        assert!(bak.exists());
        assert!(std::fs::read_to_string(&bak).unwrap().contains("other-tool"));

        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v["permissions"]["x"], 1);
        let arr = v["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert!(arr.iter().any(|e| e["hooks"][0]["command"] == "other-tool"));
    }

    #[test]
    fn install_is_idempotent() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("settings.json");
        install(&p, &exe()).unwrap();
        let after_first = std::fs::read_to_string(&p).unwrap();
        let outcome = install(&p, &exe()).unwrap();
        assert!(matches!(outcome, Outcome::AlreadyPresent));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), after_first);
    }

    #[test]
    fn install_errors_on_malformed_json_without_touching_file() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("settings.json");
        std::fs::write(&p, "{ not json").unwrap();
        assert!(install(&p, &exe()).is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ not json");
    }

    #[test]
    fn snippet_contains_abs_path_and_startup() {
        let s = snippet(&exe());
        assert!(s.contains("/usr/local/bin/wip"));
        assert!(s.contains("hook"));
        assert!(s.contains("startup"));
    }
}
