mod config;
mod uploader;
mod watcher;

use config::AppConfig;
use uploader::{UploadResult, UploadStatus, Uploader};
use watcher::{FolderWatcher, WatchEvent};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager, State,
};
use tokio::sync::{mpsc, Mutex};

const MAX_ACTIVITY_LOG: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub watching: bool,
    pub folder: Option<String>,
    pub file_count: usize,
    pub recent_activity: Vec<ActivityEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub filename: String,
    pub status: UploadStatus,
    pub error: Option<String>,
    pub timestamp: String,
}

pub struct AppState {
    config: Arc<Mutex<AppConfig>>,
    watching: Arc<Mutex<bool>>,
    activity_log: Arc<Mutex<Vec<ActivityEntry>>>,
    stop_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let config = state.config.lock().await;
    Ok(config.clone())
}

#[tauri::command]
async fn save_config(
    state: State<'_, AppState>,
    new_config: AppConfig,
) -> Result<(), String> {
    new_config.save()?;
    let mut config = state.config.lock().await;
    *config = new_config;
    Ok(())
}

#[tauri::command]
async fn select_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder.map(|f| f.to_string()));
    });

    rx.recv()
        .map_err(|e| format!("Dialog cancelled: {}", e))
}

#[tauri::command]
async fn get_sync_status(state: State<'_, AppState>) -> Result<SyncStatus, String> {
    let watching = *state.watching.lock().await;
    let config = state.config.lock().await;
    let activity = state.activity_log.lock().await;

    let file_count = config
        .watched_folder
        .as_ref()
        .and_then(|folder| count_files(folder).ok())
        .unwrap_or(0);

    Ok(SyncStatus {
        watching,
        folder: config.watched_folder.as_ref().map(|p| p.display().to_string()),
        file_count,
        recent_activity: activity.clone(),
    })
}

#[tauri::command]
async fn get_recent_activity(state: State<'_, AppState>) -> Result<Vec<ActivityEntry>, String> {
    let activity = state.activity_log.lock().await;
    Ok(activity.clone())
}

#[tauri::command]
async fn start_watching(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let config = state.config.lock().await.clone();

    if !config.is_configured() {
        return Err("App not configured. Set API URL, API key, and watched folder.".to_string());
    }

    let folder = config.watched_folder.clone().unwrap();

    if !folder.exists() {
        return Err(format!("Watched folder does not exist: {:?}", folder));
    }

    // Stop existing watcher if any
    if let Some(tx) = state.stop_tx.lock().await.take() {
        let _ = tx.send(()).await;
    }

    let (event_tx, mut event_rx) = mpsc::channel::<WatchEvent>(256);
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);

    *state.stop_tx.lock().await = Some(stop_tx);
    *state.watching.lock().await = true;

    let _watcher = FolderWatcher::start(folder, event_tx)?;

    // Spawn upload processing task
    let activity_log = state.activity_log.clone();
    let watching = state.watching.clone();
    let app_handle = app.clone();

    tokio::spawn(async move {
        let uploader = Uploader::new();
        // Keep watcher alive in this task
        let _watcher_handle = _watcher;

        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    let file_path = match &event {
                        WatchEvent::FileCreated(p) | WatchEvent::FileModified(p) => p.clone(),
                    };

                    log::info!("File event: {:?}", file_path);

                    let result = uploader.upload_and_ingest(&file_path, &config).await;
                    log_activity(&activity_log, &result).await;

                    let _ = app_handle.emit("sync-activity", &result);
                }
                _ = stop_rx.recv() => {
                    log::info!("Watcher stopped by user");
                    *watching.lock().await = false;
                    break;
                }
            }
        }
    });

    let _ = app.emit("sync-status-changed", true);

    Ok(())
}

#[tauri::command]
async fn stop_watching(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(tx) = state.stop_tx.lock().await.take() {
        let _ = tx.send(()).await;
    }
    *state.watching.lock().await = false;
    let _ = app.emit("sync-status-changed", false);
    Ok(())
}

async fn log_activity(log: &Arc<Mutex<Vec<ActivityEntry>>>, result: &UploadResult) {
    let entry = ActivityEntry {
        filename: result.filename.clone(),
        status: result.status.clone(),
        error: result.error.clone(),
        timestamp: chrono_now(),
    };

    let mut activity = log.lock().await;
    activity.insert(0, entry);
    activity.truncate(MAX_ACTIVITY_LOG);
}

fn chrono_now() -> String {
    // Simple timestamp without chrono dependency
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}

fn count_files(folder: &std::path::Path) -> Result<usize, std::io::Error> {
    let mut count = 0;
    if folder.is_dir() {
        for entry in std::fs::read_dir(folder)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                count += 1;
            } else if path.is_dir() {
                count += count_files(&path)?;
            }
        }
    }
    Ok(count)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = AppConfig::load().unwrap_or_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            select_folder,
            get_sync_status,
            get_recent_activity,
            start_watching,
            stop_watching,
        ])
        .setup(move |app| {
            // Logging
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // System tray
            let open_item = MenuItemBuilder::with_id("open", "Open").build(app)?;
            let pause_item = MenuItemBuilder::with_id("toggle", "Pause").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&open_item)
                .item(&pause_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let app_handle = app.handle().clone();
            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap())
                .menu(&menu)
                .tooltip("Exemem Client")
                .on_menu_event(move |tray_handle, event| {
                    match event.id().as_ref() {
                        "open" => {
                            if let Some(window) = tray_handle.app_handle().get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "toggle" => {
                            let _ = tray_handle.app_handle().emit("tray-toggle-watching", ());
                        }
                        "quit" => {
                            tray_handle.app_handle().exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            // Manage state
            app.manage(AppState {
                config: Arc::new(Mutex::new(config.clone())),
                watching: Arc::new(Mutex::new(false)),
                activity_log: Arc::new(Mutex::new(Vec::new())),
                stop_tx: Arc::new(Mutex::new(None)),
            });

            // Hide window on close (stay in tray)
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_clone.hide();
                    }
                });
            }

            // Auto-start watching if configured
            if config.is_configured() {
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    // Small delay to let state initialize
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if let Some(state) = handle.try_state::<AppState>() {
                        let config = state.config.lock().await.clone();
                        if config.is_configured() {
                            if let Some(folder) = &config.watched_folder {
                                let (event_tx, mut event_rx) = mpsc::channel::<WatchEvent>(256);
                                let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
                                *state.stop_tx.lock().await = Some(stop_tx);
                                *state.watching.lock().await = true;

                                match FolderWatcher::start(folder.clone(), event_tx) {
                                    Ok(_watcher) => {
                                        log::info!("Auto-started watching: {:?}", folder);
                                        let activity_log = state.activity_log.clone();
                                        let watching = state.watching.clone();
                                        let app_handle = handle.clone();

                                        tokio::spawn(async move {
                                            let uploader = Uploader::new();
                                            let _watcher_handle = _watcher;

                                            loop {
                                                tokio::select! {
                                                    Some(event) = event_rx.recv() => {
                                                        let file_path = match &event {
                                                            WatchEvent::FileCreated(p) | WatchEvent::FileModified(p) => p.clone(),
                                                        };
                                                        let result = uploader.upload_and_ingest(&file_path, &config).await;
                                                        log_activity(&activity_log, &result).await;
                                                        let _ = app_handle.emit("sync-activity", &result);
                                                    }
                                                    _ = stop_rx.recv() => {
                                                        *watching.lock().await = false;
                                                        break;
                                                    }
                                                }
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        log::error!("Failed to auto-start watcher: {}", e);
                                    }
                                }
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running exemem-client");
}
