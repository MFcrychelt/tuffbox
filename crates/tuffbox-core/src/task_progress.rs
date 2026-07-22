//! Background task progress bus (VisualTask-inspired, TuffBox-owned).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

static TASKS: LazyLock<Mutex<HashMap<String, BackgroundTask>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Running,
    Succeeded,
    Failed,
    Dismissed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTask {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    /// 0.0..=1.0 when known; None = indeterminate.
    pub progress: Option<f64>,
    pub detail: Option<String>,
    pub error: Option<String>,
    pub updated_at_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn start_task(id: impl Into<String>, title: impl Into<String>) -> String {
    let id = id.into();
    let task = BackgroundTask {
        id: id.clone(),
        title: title.into(),
        status: TaskStatus::Running,
        progress: Some(0.0),
        detail: None,
        error: None,
        updated_at_ms: now_ms(),
    };
    if let Ok(mut g) = TASKS.lock() {
        g.insert(id.clone(), task);
    }
    id
}

pub fn set_progress(id: &str, progress: f64, detail: Option<String>) {
    if let Ok(mut g) = TASKS.lock() {
        if let Some(t) = g.get_mut(id) {
            t.progress = Some(progress.clamp(0.0, 1.0));
            if let Some(d) = detail {
                t.detail = Some(d);
            }
            t.updated_at_ms = now_ms();
        }
    }
}

pub fn succeed(id: &str, detail: Option<String>) {
    if let Ok(mut g) = TASKS.lock() {
        if let Some(t) = g.get_mut(id) {
            t.status = TaskStatus::Succeeded;
            t.progress = Some(1.0);
            if let Some(d) = detail {
                t.detail = Some(d);
            }
            t.updated_at_ms = now_ms();
        }
    }
}

pub fn fail(id: &str, error: impl Into<String>) {
    if let Ok(mut g) = TASKS.lock() {
        if let Some(t) = g.get_mut(id) {
            t.status = TaskStatus::Failed;
            t.error = Some(error.into());
            t.updated_at_ms = now_ms();
        }
    }
}

pub fn dismiss(id: &str) {
    if let Ok(mut g) = TASKS.lock() {
        if let Some(t) = g.get_mut(id) {
            t.status = TaskStatus::Dismissed;
            t.updated_at_ms = now_ms();
        }
        // Drop dismissed + old succeeded after mark.
        g.retain(|_, t| {
            t.status == TaskStatus::Running
                || t.status == TaskStatus::Failed
                || (t.status == TaskStatus::Succeeded
                    && now_ms().saturating_sub(t.updated_at_ms) < 60_000)
        });
    }
}

pub fn list_tasks() -> Vec<BackgroundTask> {
    let Ok(g) = TASKS.lock() else {
        return Vec::new();
    };
    let mut v: Vec<_> = g
        .values()
        .filter(|t| t.status != TaskStatus::Dismissed)
        .cloned()
        .collect();
    v.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_lifecycle() {
        let id = start_task("test-task-1", "Demo");
        set_progress(&id, 0.5, Some("halfway".into()));
        succeed(&id, None);
        let listed = list_tasks();
        assert!(listed.iter().any(|t| t.id == id && t.status == TaskStatus::Succeeded));
        dismiss(&id);
    }
}
