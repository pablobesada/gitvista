mod benchmark;
mod editor;
mod root_badges;
mod runtime;
#[cfg(test)]
mod runtime_tests;

use gitvista_core::{
    DiffContents, DiffMode, DiffResponse, EditVersion, Preferences, WorkspaceEvent,
    WorkspaceSnapshot,
};
use runtime::Runtime;
use tauri::{ipc::Channel, Emitter, Manager, State};

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
async fn get_root_badges(
    roots: Vec<String>,
    force: bool,
    runtime: State<'_, Runtime>,
) -> Result<Vec<root_badges::RootBadge>, String> {
    runtime.root_badges(roots, force).await
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
async fn save_working_file(
    entry_id: String,
    version: EditVersion,
    contents: String,
    runtime: State<'_, Runtime>,
) -> Result<(), String> {
    runtime.save_file(entry_id, version, contents).await
}
#[tauri::command]
fn set_editor_active(active: bool, runtime: State<'_, Runtime>) {
    runtime.set_editor_active(active);
}
#[tauri::command]
fn exit_application(app: tauri::AppHandle, runtime: State<'_, Runtime>) {
    runtime.set_editor_active(false);
    app.exit(0);
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
            // macOS's predefined Quit item calls NSApplication.terminate directly.
            // Route its accelerator through our draft guard instead.
            #[cfg(target_os = "macos")]
            {
                use tauri::menu::{Menu, MenuItem};
                let menu = Menu::default(app.handle())?;
                if let Some(app_menu) = menu.items()?.first().and_then(|item| item.as_submenu()) {
                    if let Some(quit) = app_menu.items()?.last() {
                        app_menu.remove(quit)?;
                    }
                    app_menu.append(&MenuItem::with_id(
                        app,
                        "gitvista-quit",
                        "Quit GitVista",
                        true,
                        Some("CmdOrCtrl+Q"),
                    )?)?;
                }
                app.set_menu(menu)?;
            }
            Ok(())
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "gitvista-quit" {
                if app.state::<Runtime>().editor_active() {
                    let _ = app.emit("gitvista:exit-requested", ());
                } else {
                    app.exit(0);
                }
            }
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(false) = event {
                window.state::<Runtime>().cancel_root_badges();
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.state::<Runtime>().editor_active() {
                    api.prevent_close();
                    let _ = window.emit("gitvista:exit-requested", ());
                }
            }
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
            get_root_badges,
            add_repository,
            get_diff,
            load_diff_contents,
            save_working_file,
            set_editor_active,
            exit_application,
            get_preferences,
            save_preferences
        ])
        .build(tauri::generate_context!())
        .expect("Could not start GitVista")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if app.state::<Runtime>().editor_active() {
                    api.prevent_exit();
                    let _ = app.emit("gitvista:exit-requested", ());
                }
            }
        });
}
