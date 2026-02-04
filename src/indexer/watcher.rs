use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEvent, Debouncer};

use crate::error::{IndexerError, Result};

#[allow(dead_code)]
pub enum FileEvent {
    Created(std::path::PathBuf),
    Modified(std::path::PathBuf),
    Deleted(std::path::PathBuf),
}

pub struct FileWatcher {
    _debouncer: Debouncer<notify::RecommendedWatcher>,
    receiver: Receiver<std::result::Result<Vec<DebouncedEvent>, notify::Error>>,
}

impl FileWatcher {
    pub fn new(path: &Path) -> Result<Self> {
        let (tx, rx) = channel();

        let mut debouncer = new_debouncer(Duration::from_millis(500), tx)
            .map_err(|e| IndexerError::Watcher(e.to_string()))?;

        debouncer
            .watcher()
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| IndexerError::Watcher(e.to_string()))?;

        Ok(Self {
            _debouncer: debouncer,
            receiver: rx,
        })
    }

    pub fn recv(&self) -> Option<Vec<FileEvent>> {
        match self.receiver.recv() {
            Ok(Ok(events)) => {
                let file_events: Vec<FileEvent> = events
                    .into_iter()
                    .filter_map(|e| {
                        let path = e.path;
                        if path.is_file() {
                            Some(FileEvent::Modified(path))
                        } else if !path.exists() {
                            Some(FileEvent::Deleted(path))
                        } else {
                            None
                        }
                    })
                    .collect();

                if file_events.is_empty() {
                    None
                } else {
                    Some(file_events)
                }
            }
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn try_recv(&self) -> Option<Vec<FileEvent>> {
        match self.receiver.try_recv() {
            Ok(Ok(events)) => {
                let file_events: Vec<FileEvent> = events
                    .into_iter()
                    .filter_map(|e| {
                        let path = e.path;
                        if path.is_file() {
                            Some(FileEvent::Modified(path))
                        } else if !path.exists() {
                            Some(FileEvent::Deleted(path))
                        } else {
                            None
                        }
                    })
                    .collect();

                if file_events.is_empty() {
                    None
                } else {
                    Some(file_events)
                }
            }
            _ => None,
        }
    }
}
