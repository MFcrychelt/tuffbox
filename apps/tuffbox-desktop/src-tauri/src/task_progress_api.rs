//! Tauri bridge for background TaskProgress bus.

use tuffbox_core::task_progress::{self, BackgroundTask};

#[tauri::command(rename_all = "camelCase")]
pub fn list_background_tasks() -> Vec<BackgroundTask> {
    task_progress::list_tasks()
}

#[tauri::command(rename_all = "camelCase")]
pub fn dismiss_background_task(id: String) -> Result<(), String> {
    task_progress::dismiss(&id);
    Ok(())
}

/// Start a named task from the UI / other commands (returns id).
#[tauri::command(rename_all = "camelCase")]
pub fn start_background_task(id: String, title: String) -> String {
    task_progress::start_task(id, title)
}
