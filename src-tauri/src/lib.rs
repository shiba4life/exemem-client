mod config;
pub mod query;
mod scanner;
pub mod storage;
mod uploader;
mod watcher;

use config::AppConfig;
use query::QueryClient;
use scanner::{classify_single_file, ScanResult};
use uploader::{UploadResult, UploadStatus, Uploader};
use watcher::{FolderWatcher, WatchEvent};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager, State,
};
use tauri_plugin_deep_link::DeepLinkExt;
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
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileProgress {
    pub filename: String,
    pub progress_id: Option<String>,
    pub status: String,
    pub percent: f64,
    pub message: Option<String>,
}

pub struct AppState {
    config: Arc<Mutex<AppConfig>>,
    watching: Arc<Mutex<bool>>,
    activity_log: Arc<Mutex<Vec<ActivityEntry>>>,
    stop_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    scan_result: Arc<Mutex<Option<ScanResult>>>,
    ingestion_progress: Arc<Mutex<Vec<FileProgress>>>,
    query_client: QueryClient,
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

    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        let folder = app_clone.dialog().file().blocking_pick_folder();
        folder.map(|f| f.to_string())
    })
    .await
    .map_err(|e| format!("Dialog task failed: {}", e))
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
async fn scan_folder(state: State<'_, AppState>) -> Result<ScanResult, String> {
    let config = state.config.lock().await.clone();

    let folder = config
        .watched_folder
        .ok_or_else(|| "No watched folder configured".to_string())?;

    if !folder.exists() {
        return Err(format!("Folder does not exist: {:?}", folder));
    }

    let result = tokio::task::spawn_blocking(move || scanner::scan_and_classify(&folder))
        .await
        .map_err(|e| format!("Scan task failed: {}", e))??;

    *state.scan_result.lock().await = Some(result.clone());

    Ok(result)
}

#[tauri::command]
async fn approve_and_ingest(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    approved_paths: Vec<String>,
) -> Result<(), String> {
    let config = state.config.lock().await.clone();

    if !config.is_configured() {
        return Err("App not configured. Set API URL, API key, and watched folder.".to_string());
    }

    let scan_result = state.scan_result.lock().await.clone();
    let scan = scan_result.ok_or_else(|| "No scan result available. Run scan first.".to_string())?;

    // Build list of files to ingest from approved paths
    let files_to_ingest: Vec<_> = scan
        .recommended_files
        .iter()
        .chain(scan.skipped_files.iter())
        .filter(|f| approved_paths.contains(&f.path))
        .cloned()
        .collect();

    if files_to_ingest.is_empty() {
        return Err("No files selected for ingestion.".to_string());
    }

    // Initialize progress tracking
    {
        let mut progress = state.ingestion_progress.lock().await;
        *progress = files_to_ingest
            .iter()
            .map(|f| FileProgress {
                filename: f.path.clone(),
                progress_id: None,
                status: "pending".to_string(),
                percent: 0.0,
                message: None,
            })
            .collect();
    }

    // Spawn ingestion tasks
    let activity_log = state.activity_log.clone();
    let ingestion_progress = state.ingestion_progress.clone();
    let app_handle = app.clone();

    tokio::spawn(async move {
        let mut handles = Vec::new();

        for file_rec in files_to_ingest {
            let file_path = file_rec.absolute_path.clone();
            let file_name = file_rec.path.clone();
            let cfg = config.clone();
            let act_log = activity_log.clone();
            let ing_prog = ingestion_progress.clone();
            let app_h = app_handle.clone();

            let handle = tokio::spawn(async move {
                let uploader = Uploader::new();

                // Update progress to uploading
                update_file_progress(&ing_prog, &file_name, "uploading", 10.0, None).await;
                let _ = app_h.emit("ingestion-progress", get_progress_snapshot(&ing_prog).await);

                let result = uploader.upload_and_ingest(&file_path, &cfg).await;

                // Update progress based on result
                match &result.status {
                    UploadStatus::Ingesting => {
                        update_file_progress(
                            &ing_prog,
                            &file_name,
                            "ingesting",
                            50.0,
                            result.progress_id.clone(),
                        )
                        .await;

                        // Poll for completion
                        if let Some(pid) = &result.progress_id {
                            poll_until_done(&uploader, &cfg, pid, &ing_prog, &file_name, &app_h)
                                .await;
                        }
                    }
                    UploadStatus::Uploaded => {
                        update_file_progress(&ing_prog, &file_name, "uploaded", 100.0, None).await;
                    }
                    UploadStatus::Error => {
                        update_file_progress(
                            &ing_prog,
                            &file_name,
                            "error",
                            0.0,
                            None,
                        )
                        .await;
                    }
                    _ => {}
                }

                log_activity(&act_log, &result).await;
                let _ = app_h.emit("sync-activity", &result);
                let _ = app_h.emit("ingestion-progress", get_progress_snapshot(&ing_prog).await);
            });

            handles.push(handle);
        }

        // Wait for all uploads to complete
        for handle in handles {
            let _ = handle.await;
        }

        let _ = app_handle.emit("ingestion-complete", true);
    });

    Ok(())
}

async fn update_file_progress(
    progress: &Arc<Mutex<Vec<FileProgress>>>,
    filename: &str,
    status: &str,
    percent: f64,
    progress_id: Option<String>,
) {
    let mut prog = progress.lock().await;
    if let Some(entry) = prog.iter_mut().find(|p| p.filename == filename) {
        entry.status = status.to_string();
        entry.percent = percent;
        if let Some(pid) = progress_id {
            entry.progress_id = Some(pid);
        }
    }
}

async fn get_progress_snapshot(progress: &Arc<Mutex<Vec<FileProgress>>>) -> Vec<FileProgress> {
    progress.lock().await.clone()
}

async fn poll_until_done(
    uploader: &Uploader,
    config: &AppConfig,
    progress_id: &str,
    progress: &Arc<Mutex<Vec<FileProgress>>>,
    filename: &str,
    app: &tauri::AppHandle,
) {
    let max_polls = 120; // 4 minutes at 2s intervals
    for _ in 0..max_polls {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        match uploader.poll_progress(config, progress_id).await {
            Ok(resp) => {
                let percent = resp.percent.unwrap_or(50.0);
                let status = resp.status.as_str();

                {
                    let mut prog = progress.lock().await;
                    if let Some(entry) = prog.iter_mut().find(|p| p.filename == filename) {
                        entry.status = status.to_string();
                        entry.percent = percent;
                        entry.message = resp.message.clone();
                    }
                }

                let _ = app.emit("ingestion-progress", get_progress_snapshot(progress).await);

                if status == "completed" || status == "done" || status == "error" || status == "failed" {
                    if status == "completed" || status == "done" {
                        update_file_progress(progress, filename, "done", 100.0, None).await;
                    }
                    break;
                }
            }
            Err(e) => {
                log::warn!("Progress poll error for {}: {}", filename, e);
                // Don't break on poll errors, just keep trying
            }
        }
    }
}

#[tauri::command]
async fn get_ingestion_progress(
    state: State<'_, AppState>,
) -> Result<Vec<FileProgress>, String> {
    let progress = state.ingestion_progress.lock().await;
    Ok(progress.clone())
}

#[tauri::command]
async fn run_query(
    state: State<'_, AppState>,
    query: String,
    session_id: Option<String>,
) -> Result<query::RunQueryResponse, String> {
    let config = state.config.lock().await.clone();
    state
        .query_client
        .run_query(&config, &query, session_id.as_deref())
        .await
}

#[tauri::command]
async fn chat_followup(
    state: State<'_, AppState>,
    session_id: String,
    question: String,
) -> Result<query::ChatResponse, String> {
    let config = state.config.lock().await.clone();
    state
        .query_client
        .chat_followup(&config, &session_id, &question)
        .await
}

#[tauri::command]
async fn search_index(
    state: State<'_, AppState>,
    term: String,
) -> Result<query::SearchResponse, String> {
    let config = state.config.lock().await.clone();
    state.query_client.search_index(&config, &term).await
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

    let _watcher = FolderWatcher::start(folder.clone(), event_tx)?;

    // Spawn upload processing task
    let activity_log = state.activity_log.clone();
    let watching = state.watching.clone();
    let app_handle = app.clone();
    let auto_approve = config.auto_approve_watched;

    tokio::spawn(async move {
        let uploader = Uploader::new();
        let _watcher_handle = _watcher;

        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    let file_path = match &event {
                        WatchEvent::FileCreated(p) | WatchEvent::FileModified(p) => p.clone(),
                    };

                    log::info!("File event: {:?}", file_path);

                    // Classify the new file
                    let recommendation = classify_single_file(&folder, &file_path);

                    // Emit classification info to frontend
                    let _ = app_handle.emit("new-file-detected", &recommendation);

                    if auto_approve && recommendation.should_ingest {
                        let result = uploader.upload_and_ingest(&file_path, &config).await;
                        log_activity_with_category(&activity_log, &result, Some(recommendation.category)).await;
                        let _ = app_handle.emit("sync-activity", &result);
                    } else {
                        // Log as skipped
                        let entry = ActivityEntry {
                            filename: recommendation.path,
                            status: UploadStatus::Uploaded, // Not uploaded, just detected
                            error: if recommendation.should_ingest {
                                Some("Waiting for approval".to_string())
                            } else {
                                Some(format!("Skipped ({})", recommendation.category))
                            },
                            timestamp: chrono_now(),
                            category: Some(recommendation.category),
                        };
                        let mut activity = activity_log.lock().await;
                        activity.insert(0, entry.clone());
                        activity.truncate(MAX_ACTIVITY_LOG);
                        let _ = app_handle.emit("sync-activity", &entry);
                    }
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
    log_activity_with_category(log, result, None).await;
}

async fn log_activity_with_category(
    log: &Arc<Mutex<Vec<ActivityEntry>>>,
    result: &UploadResult,
    category: Option<String>,
) {
    let entry = ActivityEntry {
        filename: result.filename.clone(),
        status: result.status.clone(),
        error: result.error.clone(),
        timestamp: chrono_now(),
        category,
    };

    let mut activity = log.lock().await;
    activity.insert(0, entry);
    activity.truncate(MAX_ACTIVITY_LOG);
}

fn chrono_now() -> String {
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

/// Process a deep link URL and emit auth data to the frontend
fn handle_deep_link_url(app: &tauri::AppHandle, url: &url::Url) {
    log::info!("Processing deep link: {}", url);

    // exemem://auth/callback?api_key=...&user_hash=...&session_token=...
    if url.host_str() == Some("auth") {
        let params: std::collections::HashMap<String, String> =
            url.query_pairs().into_owned().collect();

        let payload = serde_json::json!({
            "api_key": params.get("api_key"),
            "user_hash": params.get("user_hash"),
            "session_token": params.get("session_token"),
        });

        log::info!("Deep link auth callback received");
        let _ = app.emit("deep-link-auth", payload);

        // Bring window to front
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = AppConfig::load().unwrap_or_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            select_folder,
            get_sync_status,
            get_recent_activity,
            scan_folder,
            approve_and_ingest,
            get_ingestion_progress,
            run_query,
            chat_followup,
            search_index,
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

            // Deep link handling
            #[cfg(any(windows, target_os = "linux"))]
            {
                let _ = app.deep_link().register_all();
            }

            if let Ok(Some(urls)) = app.deep_link().get_current() {
                for url in &urls {
                    handle_deep_link_url(app.handle(), url);
                }
            }

            let deep_link_handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    handle_deep_link_url(&deep_link_handle, &url);
                }
            });

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
                scan_result: Arc::new(Mutex::new(None)),
                ingestion_progress: Arc::new(Mutex::new(Vec::new())),
                query_client: QueryClient::new(),
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

                                let folder_clone = folder.clone();
                                match FolderWatcher::start(folder.clone(), event_tx) {
                                    Ok(_watcher) => {
                                        log::info!("Auto-started watching: {:?}", folder);
                                        let activity_log = state.activity_log.clone();
                                        let watching = state.watching.clone();
                                        let app_handle = handle.clone();
                                        let auto_approve = config.auto_approve_watched;

                                        tokio::spawn(async move {
                                            let uploader = Uploader::new();
                                            let _watcher_handle = _watcher;

                                            loop {
                                                tokio::select! {
                                                    Some(event) = event_rx.recv() => {
                                                        let file_path = match &event {
                                                            WatchEvent::FileCreated(p) | WatchEvent::FileModified(p) => p.clone(),
                                                        };

                                                        let recommendation = classify_single_file(&folder_clone, &file_path);
                                                        let _ = app_handle.emit("new-file-detected", &recommendation);

                                                        if auto_approve && recommendation.should_ingest {
                                                            let result = uploader.upload_and_ingest(&file_path, &config).await;
                                                            log_activity_with_category(&activity_log, &result, Some(recommendation.category)).await;
                                                            let _ = app_handle.emit("sync-activity", &result);
                                                        }
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
