//! Quest AI chat sessions under `.tuffbox/quest_chats/`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::quest_plan::QuestPlan;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Attached QuestPlan when the assistant produced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<QuestPlan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_log: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestChatSession {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub messages: Vec<QuestChatMessage>,
    /// Last pending plan awaiting Apply in the editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_plan: Option<QuestPlan>,
    #[serde(default)]
    pub updated_at: String,
}

pub fn quest_chats_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("quest_chats")
}

pub fn list_quest_chats(project_dir: &Path) -> Result<Vec<QuestChatSession>, String> {
    let dir = quest_chats_dir(project_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if let Ok(session) = serde_json::from_str::<QuestChatSession>(&text) {
            sessions.push(session);
        }
    }
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

pub fn save_quest_chat(
    project_dir: &Path,
    session: &QuestChatSession,
) -> Result<PathBuf, String> {
    let dir = quest_chats_dir(project_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", session.id));
    let text = serde_json::to_string_pretty(session).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn load_quest_chat(project_dir: &Path, chat_id: &str) -> Result<QuestChatSession, String> {
    let path = quest_chats_dir(project_dir).join(format!("{chat_id}.json"));
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub fn delete_quest_chat(project_dir: &Path, chat_id: &str) -> Result<(), String> {
    let path = quest_chats_dir(project_dir).join(format!("{chat_id}.json"));
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn new_quest_chat_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("qchat-{ms}")
}

pub fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
