//! File system watcher for automatic memory reindexing

use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tracing::{debug, info, warn};

use super::MemoryIndex;
use crate::config::MemoryConfig;

pub struct MemoryWatcher {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    #[allow(dead_code)]
    workspace: PathBuf,
}

impl MemoryWatcher {
    pub fn new(workspace: PathBuf, _config: MemoryConfig) -> Result<Self> {
        let _workspace_clone = workspace.clone();

        // Create a channel for receiving events
        let (tx, rx) = mpsc::channel();

        // Create watcher with debounce
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    // Filter for modify/create events on .md files
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            for path in event.paths {
                                if path.extension().map(|e| e == "md").unwrap_or(false) {
                                    if let Err(e) = tx.send(path.clone()) {
                                        warn!("Failed to send event: {}", e);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Err(e) => warn!("Watch error: {:?}", e),
            }
        })?;

        // Watch the workspace directory
        watcher.watch(&workspace, RecursiveMode::Recursive)?;

        info!("Started watching memory files in: {}", workspace.display());

        // Spawn background task to handle events
        let workspace_for_task = workspace.clone();
        std::thread::spawn(move || {
            let index = match MemoryIndex::new(&workspace_for_task) {
                Ok(idx) => idx,
                Err(e) => {
                    warn!("Failed to create memory index for watcher: {}", e);
                    return;
                }
            };

            // Debounce events
            let debounce_duration = Duration::from_secs(2);

            loop {
                match rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(path) => {
                        debug!("File changed: {}", path.display());

                        // Debounce: wait for events to settle
                        let mut last_event_time = std::time::Instant::now();
                        while last_event_time.elapsed() < debounce_duration {
                            match rx.recv_timeout(debounce_duration - last_event_time.elapsed()) {
                                Ok(p) => {
                                    debug!("Additional file changed: {}", p.display());
                                    last_event_time = std::time::Instant::now();
                                }
                                Err(mpsc::RecvTimeoutError::Timeout) => break,
                                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                            }
                        }

                        // Reindex the file
                        if let Err(e) = index.index_file(&path, false) {
                            warn!("Failed to reindex file {}: {}", path.display(), e);
                        } else {
                            info!("Reindexed: {}", path.display());
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        info!("Watcher channel disconnected");
                        return;
                    }
                }
            }
        });

        Ok(Self { watcher, workspace })
    }
}
