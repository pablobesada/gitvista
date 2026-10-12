mod benchmark;
mod runtime;
#[cfg(test)]
mod runtime_tests;

use gitvista_core::{
    DiffContents, DiffMode, DiffResponse, Preferences, WorkspaceEvent, WorkspaceSnapshot,
};
use runtime::Runtime;
use tauri::{ipc::Channel, Manager, State};

#[tauri::command]
async fn open_workspace(
    root: String,
    on_event: Channel<WorkspaceEvent>,
    runtime: State<'_, Runtime>,
) -> Result<WorkspaceSnapshot, String> {
    runtime.open(root, on_event).await
}
#[tauri::command]
fn close_workspace(runtime: State<'_, Runtime>) -> Result<(), String> {
    runtime.close()
}
#[tauri::command]
async fn refresh_workspace(runtime: State<'_, Runtime>) -> Result<(), String> {
    runtime.refresh(true).await
}
#[tauri::command]
async fn add_repository(path: String, runtime: State<'_, Runtime>) -> Result<(), String> {
    runtime.add_repository(path).await
}
#[tauri::command]
async fn get_diff(
    entry_id: String,
    mode: DiffMode,
    request_id: u64,
    runtime: State<'_, Runtime>,
) -> Result<DiffResponse, String> {
    runtime.diff(entry_id, mode, request_id).await
}
#[tauri::command]
async fn load_diff_contents(
    entry_id: String,
    mode: DiffMode,
    request_id: u64,
    runtime: State<'_, Runtime>,
) -> Result<DiffContents, String> {
    runtime.contents(entry_id, mode, request_id).await
}
#[tauri::command]
fn get_preferences(runtime: State<'_, Runtime>) -> Preferences {
    runtime.preferences()
}
#[tauri::command]
fn save_preferences(preferences: Preferences, runtime: State<'_, Runtime>) -> Result<(), String> {
    runtime.save_preferences(preferences)
}

pub fn run() {
    if benchmark::try_run() {
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("preferences.json");
            let runtime = Runtime::new(path);
            app.manage(runtime.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(true) = event {
                let runtime = window.state::<Runtime>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = runtime.refresh(false).await;
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            open_workspace,
            close_workspace,
            refresh_workspace,
            add_repository,
            get_diff,
            load_diff_contents,
            get_preferences,
            save_preferences
        ])
        .run(tauri::generate_context!())
        .expect("Could not start GitVista");
}
