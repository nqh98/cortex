use crate::config::Config;
use crate::indexer::Indexer;
use crate::models::Language;
use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

pub async fn watch_project(project_path: &Path, config: &Config) -> crate::error::Result<()> {
    let indexer = Indexer::new(config).await?;
    let (tx, rx) = mpsc::channel();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        NotifyConfig::default(),
    )
    .map_err(|e| crate::error::CortexError::Watcher(e.to_string()))?;

    watcher
        .watch(project_path, RecursiveMode::Recursive)
        .map_err(|e| crate::error::CortexError::Watcher(e.to_string()))?;

    info!("Watching {} for changes...", project_path.display());

    let debounce = Duration::from_millis(config.watcher.debounce_ms);
    let mut last_event = Instant::now() - debounce;
    let mut pending: Vec<std::path::PathBuf> = Vec::new();

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                let now = Instant::now();
                if now - last_event < debounce {
                    continue;
                }
                last_event = now;

                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        for path in &event.paths {
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                if Language::from_extension(ext).is_some() {
                                    pending.push(path.clone());
                                }
                            }
                        }
                    }
                    EventKind::Remove(_) => {
                        for path in &event.paths {
                            info!("File removed: {}", path.display());
                        }
                    }
                    _ => {}
                }

                // Process pending files
                if !pending.is_empty() {
                    for path in pending.drain(..) {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        if let Some(lang) = Language::from_extension(ext) {
                            match indexer.index_single_file(project_path, &path, lang).await {
                                Ok(n) => info!("Re-indexed {}: {} symbols", path.display(), n),
                                Err(e) => warn!("Failed to re-index {}: {}", path.display(), e),
                            }
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                error!("File watcher disconnected");
                break;
            }
        }
    }

    Ok(())
}
