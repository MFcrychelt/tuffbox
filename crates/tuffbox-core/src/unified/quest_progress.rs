//! Read-only FTB Quests team progress from world saves.
//!
//! On-disk layout (per [FTBTeam docs](https://github.com/FTBTeam/FTB-Mods-Issues/issues/1991)):
//! `saves/<world>/ftbquests/<teamUuid>.snbt` (legacy) or `.json5` (modern).
//!
//! We only surface completion / started / locked for the canvas overlay —
//! no write-back in Phase C.

use crate::unified::quest_book::{QuestBook, Quest};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestProgressTeamRef {
    pub world: String,
    pub team_id: String,
    pub name: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuestProgressStatus {
    Completed,
    Started,
    Available,
    Locked,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QuestProgressSnapshot {
    pub world: String,
    pub team_id: String,
    pub name: String,
    /// questId → status
    pub statuses: HashMap<String, QuestProgressStatus>,
    pub completed_count: usize,
    pub started_count: usize,
}

/// List team progress files under `saves/*/ftbquests/`.
pub fn list_progress_teams(project_dir: &Path) -> Vec<QuestProgressTeamRef> {
    let saves = project_dir.join("saves");
    let mut out = Vec::new();
    let Ok(worlds) = std::fs::read_dir(&saves) else {
        return out;
    };
    for world_ent in worlds.flatten() {
        let world_path = world_ent.path();
        if !world_path.is_dir() {
            continue;
        }
        let world_name = world_ent.file_name().to_string_lossy().to_string();
        let ftbq = world_path.join("ftbquests");
        if !ftbq.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&ftbq) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !matches!(ext.as_str(), "snbt" | "json5" | "json") {
                continue;
            }
            let team_id = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if team_id.is_empty() {
                continue;
            }
            let name = peek_team_name(&path).unwrap_or_else(|| short_uuid(&team_id));
            let relative_path = path
                .strip_prefix(project_dir)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            out.push(QuestProgressTeamRef {
                world: world_name.clone(),
                team_id,
                name,
                relative_path,
            });
        }
    }
    out.sort_by(|a, b| {
        a.world
            .cmp(&b.world)
            .then(a.name.cmp(&b.name))
            .then(a.team_id.cmp(&b.team_id))
    });
    out
}

/// Load progress for one team file and map it onto the given quest book.
pub fn load_progress_for_book(
    project_dir: &Path,
    relative_path: &str,
    book: &QuestBook,
) -> Result<QuestProgressSnapshot, String> {
    let path = project_dir.join(relative_path);
    if !path.is_file() {
        return Err(format!("progress file not found: {relative_path}"));
    }
    let raw = parse_progress_file(&path)?;
    let team_id = raw
        .get("uuid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        });
    let name = raw
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| short_uuid(&team_id));
    let world = path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let completed_keys = collect_id_keys(raw.get("completed"));
    let task_keys = collect_id_keys(
        raw
            .get("task_progress")
            .or_else(|| raw.get("taskProgress")),
    );

    let mut statuses = HashMap::new();
    let mut completed_count = 0usize;
    let mut started_count = 0usize;

    for ch in &book.chapters {
        for q in &ch.quests {
            let status = classify_quest(q, book, &completed_keys, &task_keys);
            match status {
                QuestProgressStatus::Completed => completed_count += 1,
                QuestProgressStatus::Started => started_count += 1,
                _ => {}
            }
            statuses.insert(q.id.clone(), status);
        }
    }

    Ok(QuestProgressSnapshot {
        world,
        team_id,
        name,
        statuses,
        completed_count,
        started_count,
    })
}

fn classify_quest(
    q: &Quest,
    book: &QuestBook,
    completed: &HashSet<String>,
    task_progress: &HashSet<String>,
) -> QuestProgressStatus {
    if id_matches_any(&q.id, completed) {
        return QuestProgressStatus::Completed;
    }
    let owners = book.task_owner_map();
    let deps_ok = q.dependencies.iter().all(|d| {
        if id_matches_any(d, completed) || id_matches_any(d, task_progress) {
            return true;
        }
        // Dep points at a task whose parent quest is completed.
        if let Some(owner) = owners.get(d) {
            return id_matches_any(owner, completed);
        }
        false
    });
    if !deps_ok {
        return QuestProgressStatus::Locked;
    }
    // Reuse task_progress set: any started task on this quest.
    let started = q.tasks.iter().any(|t| id_matches_any(&t.id, task_progress));
    if started {
        return QuestProgressStatus::Started;
    }
    QuestProgressStatus::Available
}

fn id_matches_any(id: &str, keys: &HashSet<String>) -> bool {
    for k in id_key_variants(id) {
        if keys.contains(&k) {
            return true;
        }
    }
    false
}

fn id_key_variants(id: &str) -> Vec<String> {
    let mut out = vec![id.to_string(), id.to_ascii_lowercase(), id.to_ascii_uppercase()];
    let hex = id.trim_start_matches("0x");
    if let Ok(n) = u64::from_str_radix(hex, 16) {
        out.push(n.to_string());
        out.push((n as i64).to_string());
        out.push(format!("{n:x}"));
        out.push(format!("{n:X}"));
        out.push(format!("{n:016x}"));
        out.push(format!("{n:016X}"));
    }
    if let Ok(n) = id.parse::<i64>() {
        out.push(n.to_string());
        out.push(format!("{:x}", n as u64));
        out.push(format!("{:016X}", n as u64));
    }
    out
}

/// Flatten map-like JSON into a set of normalized key strings (values > 0).
fn collect_id_keys(v: Option<&serde_json::Value>) -> HashSet<String> {
    let mut set = HashSet::new();
    let Some(v) = v else {
        return set;
    };
    if let Some(obj) = v.as_object() {
        for (k, val) in obj {
            let active = match val {
                serde_json::Value::Bool(b) => *b,
                serde_json::Value::Number(n) => n.as_i64().unwrap_or(0) != 0
                    || n.as_u64().unwrap_or(0) != 0
                    || n.as_f64().unwrap_or(0.0) != 0.0,
                serde_json::Value::String(s) => !s.is_empty() && s != "0",
                serde_json::Value::Null => false,
                _ => true,
            };
            if active {
                for variant in id_key_variants(k) {
                    set.insert(variant);
                }
            }
        }
    }
    set
}

fn peek_team_name(path: &Path) -> Option<String> {
    let raw = parse_progress_file(path).ok()?;
    raw.get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn short_uuid(id: &str) -> String {
    let clean = id.replace('-', "");
    if clean.len() >= 8 {
        format!("{}…", &clean[..8])
    } else {
        id.to_string()
    }
}

fn parse_progress_file(path: &Path) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext == "snbt" {
        return crate::unified::quest_book::snbt_to_json(&text);
    }
    // json / json5 — strip comments + trailing commas, then JSON.
    let cleaned = strip_json5ish(&text);
    serde_json::from_str(&cleaned).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn strip_json5ish(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut in_str = false;
    let mut str_ch = b'"';
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            out.push(c as char);
            if c == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == str_ch {
                in_str = false;
            }
            i += 1;
            continue;
        }
        // line comment
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // block comment
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if c == b'"' || c == b'\'' {
            in_str = true;
            str_ch = c;
            // normalize to double quotes for JSON
            out.push('"');
            i += 1;
            continue;
        }
        // trailing comma before } or ]
        if c == b',' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'}' || bytes[j] == b']') {
                i += 1;
                continue;
            }
        }
        // unquoted keys: copy as-is if already JSON-like; leave numbers/idents
        out.push(c as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unified::quest_book::{Chapter, Quest, Task};
    use std::io::Write;
    use std::path::PathBuf;

    fn sample_book() -> QuestBook {
        let mk = |id: &str, deps: &[&str]| Quest {
            id: id.into(),
            title: id.into(),
            subtitle: None,
            description: vec![],
            x: 0.0,
            y: 0.0,
            icon: None,
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            tasks: vec![Task {
                id: format!("t_{id}"),
                task_type: "checkmark".into(),
                title: None,
                value: None,
                properties: Default::default(),
            }],
            rewards: vec![],
            optional: false,
            shape: None,
            size: None,
            hide_dependency_lines: None,
            hide_dependent_lines: None,
            min_required_dependencies: None,
            can_repeat: None,
            invisible: None,
            disable_toast: None,
            dependency_requirement: None,
            extras: Default::default(),
        };
        QuestBook {
            chapters: vec![Chapter {
                id: "c".into(),
                title: "C".into(),
                icon: None,
                quests: vec![mk("AAAA", &[]), mk("BBBB", &["AAAA"]), mk("CCCC", &["BBBB"])],
                group: None,
                order_index: None,
                filename: None,
                default_quest_shape: None,
                default_hide_dependency_lines: None,
                extras: Default::default(),
                source_file: None,
            }],
            title: None,
            subtitle: None,
            chapter_groups: vec![],
            reward_tables: vec![],
            book_settings: Default::default(),
        }
    }

    #[test]
    fn classifies_completed_and_locked() {
        let dir = tempfile_dir();
        let world = dir.join("saves").join("TestWorld").join("ftbquests");
        std::fs::create_dir_all(&world).unwrap();
        let file = world.join("11111111-1111-1111-1111-111111111111.snbt");
        let mut f = std::fs::File::create(&file).unwrap();
        write!(
            f,
            r#"{{ uuid: "11111111-1111-1111-1111-111111111111" name: "Steve" completed: {{ AAAA: 1L }} task_progress: {{}} }}"#
        )
        .unwrap();

        let book = sample_book();
        let rel = file
            .strip_prefix(&dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let snap = load_progress_for_book(&dir, &rel, &book).unwrap();
        assert_eq!(snap.name, "Steve");
        assert_eq!(
            snap.statuses.get("AAAA"),
            Some(&QuestProgressStatus::Completed)
        );
        assert_eq!(
            snap.statuses.get("BBBB"),
            Some(&QuestProgressStatus::Available)
        );
        assert_eq!(
            snap.statuses.get("CCCC"),
            Some(&QuestProgressStatus::Locked)
        );
    }

    fn tempfile_dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "tuffbox-qprog-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
