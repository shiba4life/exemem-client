use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const DEBOUNCE_MS: u64 = 500;

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "json", "csv", "txt", "md", "js", "ts", "jsx", "tsx", "pdf", "png", "jpg", "jpeg", "gif",
    "svg", "html", "xml", "yaml", "yml", "toml", "log", "doc", "docx", "xls", "xlsx", "ppt",
    "pptx", "rtf",
];

#[derive(Debug, Clone)]
pub enum WatchEvent {
    FileCreated(PathBuf),
    FileModified(PathBuf),
}

pub struct FolderWatcher {
    _watcher: RecommendedWatcher,
}

impl FolderWatcher {
    pub fn start(
        folder: PathBuf,
        tx: mpsc::Sender<WatchEvent>,
    ) -> Result<Self, String> {
        let (notify_tx, notify_rx) = std::sync::mpsc::channel();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = notify_tx.send(event);
                }
            },
            notify::Config::default(),
        )
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

        watcher
            .watch(&folder, RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch folder: {}", e))?;

        // Spawn debounce + filter thread
        tokio::task::spawn_blocking(move || {
            debounce_loop(notify_rx, tx);
        });

        log::info!("Watching folder: {:?}", folder);

        Ok(Self { _watcher: watcher })
    }
}

pub fn is_supported(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn debounce_loop(
    rx: std::sync::mpsc::Receiver<Event>,
    tx: mpsc::Sender<WatchEvent>,
) {
    let mut last_seen: HashMap<PathBuf, Instant> = HashMap::new();
    let debounce = Duration::from_millis(DEBOUNCE_MS);

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                for path in event.paths {
                    if !is_supported(&path) {
                        continue;
                    }

                    // Skip directories
                    if path.is_dir() {
                        continue;
                    }

                    let now = Instant::now();
                    if let Some(last) = last_seen.get(&path) {
                        if now.duration_since(*last) < debounce {
                            continue;
                        }
                    }
                    last_seen.insert(path.clone(), now);

                    let watch_event = match event.kind {
                        EventKind::Create(_) => WatchEvent::FileCreated(path),
                        EventKind::Modify(_) => WatchEvent::FileModified(path),
                        _ => continue,
                    };

                    if tx.blocking_send(watch_event).is_err() {
                        log::error!("Watch event channel closed");
                        return;
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                log::info!("Watcher disconnected");
                return;
            }
        }
    }
}
