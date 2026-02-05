//! Write Queue for SQLite single-writer serialization.
//!
//! SQLite WAL mode allows only one writer at a time. This module provides
//! application-level write serialization using tokio mpsc channels to prevent
//! SQLITE_BUSY errors in concurrent scenarios.

use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

use crate::docs::{ConfigDigest, DocDigest};
use crate::error::Result;
use crate::index::sqlite::SqliteIndex;
use crate::index::{CodeIndex, FileTag, Scope, Symbol, SymbolMetrics};
use crate::indexer::ExtractionResult;

/// Commands that can be sent to the write queue worker.
#[derive(Debug)]
pub enum WriteCommand {
    /// Add symbols to the index
    AddSymbols {
        symbols: Vec<Symbol>,
        respond: oneshot::Sender<Result<()>>,
    },
    /// Remove a file from the index
    RemoveFile {
        file_path: String,
        respond: oneshot::Sender<Result<()>>,
    },
    /// Remove multiple files from the index
    RemoveFilesBatch {
        file_paths: Vec<String>,
        respond: oneshot::Sender<Result<()>>,
    },
    /// Add extraction results in batch
    AddExtractionResults {
        results: Vec<ExtractionResult>,
        respond: oneshot::Sender<Result<usize>>,
    },
    /// Set file content hash
    SetFileContentHash {
        file_path: String,
        content_hash: String,
        respond: oneshot::Sender<Result<()>>,
    },
    /// Add scopes
    AddScopes {
        scopes: Vec<Scope>,
        respond: oneshot::Sender<Result<()>>,
    },
    /// Update symbol metrics batch
    UpdateSymbolMetricsBatch {
        metrics: Vec<SymbolMetrics>,
        respond: oneshot::Sender<Result<()>>,
    },
    /// Add file tags
    AddFileTags {
        file_path: String,
        tags: Vec<FileTag>,
        respond: oneshot::Sender<Result<()>>,
    },
    /// Add doc digests batch
    AddDocDigestsBatch {
        digests: Vec<DocDigest>,
        respond: oneshot::Sender<Result<()>>,
    },
    /// Add config digest
    AddConfigDigest {
        digest: ConfigDigest,
        respond: oneshot::Sender<Result<()>>,
    },
    /// Clear the entire index
    Clear {
        respond: oneshot::Sender<Result<()>>,
    },
    /// Shutdown the write queue worker
    Shutdown,
}

/// Handle for sending write commands to the queue.
/// This is cheaply cloneable and can be shared across tasks.
#[derive(Clone)]
pub struct WriteQueueHandle {
    sender: mpsc::Sender<WriteCommand>,
}

impl WriteQueueHandle {
    /// Default channel buffer size
    const DEFAULT_BUFFER_SIZE: usize = 256;

    /// Creates a new write queue handle and spawns the worker task.
    pub fn new(index: Arc<SqliteIndex>) -> Self {
        Self::with_buffer_size(index, Self::DEFAULT_BUFFER_SIZE)
    }

    /// Creates a new write queue handle with a custom buffer size.
    pub fn with_buffer_size(index: Arc<SqliteIndex>, buffer_size: usize) -> Self {
        let (sender, receiver) = mpsc::channel(buffer_size);
        let worker = WriteQueueWorker::new(receiver, index);

        // Spawn the worker task
        tokio::spawn(async move {
            worker.run().await;
        });

        Self { sender }
    }

    /// Adds symbols to the index.
    pub async fn add_symbols(&self, symbols: Vec<Symbol>) -> Result<()> {
        let (respond, rx) = oneshot::channel();
        self.sender
            .send(WriteCommand::AddSymbols { symbols, respond })
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue closed".into()))?;
        rx.await
            .map_err(|_| crate::error::IndexerError::Index("Write response channel closed".into()))?
    }

    /// Removes a file from the index.
    pub async fn remove_file(&self, file_path: String) -> Result<()> {
        let (respond, rx) = oneshot::channel();
        self.sender
            .send(WriteCommand::RemoveFile { file_path, respond })
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue closed".into()))?;
        rx.await
            .map_err(|_| crate::error::IndexerError::Index("Write response channel closed".into()))?
    }

    /// Removes multiple files from the index.
    pub async fn remove_files_batch(&self, file_paths: Vec<String>) -> Result<()> {
        let (respond, rx) = oneshot::channel();
        self.sender
            .send(WriteCommand::RemoveFilesBatch { file_paths, respond })
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue closed".into()))?;
        rx.await
            .map_err(|_| crate::error::IndexerError::Index("Write response channel closed".into()))?
    }

    /// Adds extraction results in batch.
    pub async fn add_extraction_results(&self, results: Vec<ExtractionResult>) -> Result<usize> {
        let (respond, rx) = oneshot::channel();
        self.sender
            .send(WriteCommand::AddExtractionResults { results, respond })
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue closed".into()))?;
        rx.await
            .map_err(|_| crate::error::IndexerError::Index("Write response channel closed".into()))?
    }

    /// Sets the content hash for a file.
    pub async fn set_file_content_hash(&self, file_path: String, content_hash: String) -> Result<()> {
        let (respond, rx) = oneshot::channel();
        self.sender
            .send(WriteCommand::SetFileContentHash {
                file_path,
                content_hash,
                respond,
            })
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue closed".into()))?;
        rx.await
            .map_err(|_| crate::error::IndexerError::Index("Write response channel closed".into()))?
    }

    /// Adds scopes to the index.
    pub async fn add_scopes(&self, scopes: Vec<Scope>) -> Result<()> {
        let (respond, rx) = oneshot::channel();
        self.sender
            .send(WriteCommand::AddScopes { scopes, respond })
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue closed".into()))?;
        rx.await
            .map_err(|_| crate::error::IndexerError::Index("Write response channel closed".into()))?
    }

    /// Updates symbol metrics in batch.
    pub async fn update_symbol_metrics_batch(&self, metrics: Vec<SymbolMetrics>) -> Result<()> {
        let (respond, rx) = oneshot::channel();
        self.sender
            .send(WriteCommand::UpdateSymbolMetricsBatch { metrics, respond })
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue closed".into()))?;
        rx.await
            .map_err(|_| crate::error::IndexerError::Index("Write response channel closed".into()))?
    }

    /// Adds file tags.
    pub async fn add_file_tags(&self, file_path: String, tags: Vec<FileTag>) -> Result<()> {
        let (respond, rx) = oneshot::channel();
        self.sender
            .send(WriteCommand::AddFileTags {
                file_path,
                tags,
                respond,
            })
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue closed".into()))?;
        rx.await
            .map_err(|_| crate::error::IndexerError::Index("Write response channel closed".into()))?
    }

    /// Adds doc digests in batch.
    pub async fn add_doc_digests_batch(&self, digests: Vec<DocDigest>) -> Result<()> {
        let (respond, rx) = oneshot::channel();
        self.sender
            .send(WriteCommand::AddDocDigestsBatch { digests, respond })
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue closed".into()))?;
        rx.await
            .map_err(|_| crate::error::IndexerError::Index("Write response channel closed".into()))?
    }

    /// Adds a config digest.
    pub async fn add_config_digest(&self, digest: ConfigDigest) -> Result<()> {
        let (respond, rx) = oneshot::channel();
        self.sender
            .send(WriteCommand::AddConfigDigest { digest, respond })
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue closed".into()))?;
        rx.await
            .map_err(|_| crate::error::IndexerError::Index("Write response channel closed".into()))?
    }

    /// Clears the entire index.
    pub async fn clear(&self) -> Result<()> {
        let (respond, rx) = oneshot::channel();
        self.sender
            .send(WriteCommand::Clear { respond })
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue closed".into()))?;
        rx.await
            .map_err(|_| crate::error::IndexerError::Index("Write response channel closed".into()))?
    }

    /// Shuts down the write queue worker.
    /// After shutdown, all subsequent write operations will fail.
    pub async fn shutdown(&self) -> Result<()> {
        self.sender
            .send(WriteCommand::Shutdown)
            .await
            .map_err(|_| crate::error::IndexerError::Index("Write queue already closed".into()))?;
        Ok(())
    }

    /// Checks if the write queue is still active.
    pub fn is_active(&self) -> bool {
        !self.sender.is_closed()
    }
}

/// Worker that processes write commands sequentially.
pub struct WriteQueueWorker {
    receiver: mpsc::Receiver<WriteCommand>,
    index: Arc<SqliteIndex>,
}

impl WriteQueueWorker {
    /// Creates a new write queue worker.
    fn new(receiver: mpsc::Receiver<WriteCommand>, index: Arc<SqliteIndex>) -> Self {
        Self { receiver, index }
    }

    /// Runs the worker loop, processing commands until shutdown.
    async fn run(mut self) {
        tracing::debug!("WriteQueue worker started");

        while let Some(command) = self.receiver.recv().await {
            match command {
                WriteCommand::AddSymbols { symbols, respond } => {
                    let result = self.index.add_symbols(symbols);
                    let _ = respond.send(result);
                }
                WriteCommand::RemoveFile { file_path, respond } => {
                    let result = self.index.remove_file(&file_path);
                    let _ = respond.send(result);
                }
                WriteCommand::RemoveFilesBatch { file_paths, respond } => {
                    let file_refs: Vec<&str> = file_paths.iter().map(|s| s.as_str()).collect();
                    let result = self.index.remove_files_batch(&file_refs);
                    let _ = respond.send(result);
                }
                WriteCommand::AddExtractionResults { results, respond } => {
                    let result = self.index.add_extraction_results_batch(results);
                    let _ = respond.send(result);
                }
                WriteCommand::SetFileContentHash {
                    file_path,
                    content_hash,
                    respond,
                } => {
                    let result = self.index.set_file_content_hash(&file_path, &content_hash);
                    let _ = respond.send(result);
                }
                WriteCommand::AddScopes { scopes, respond } => {
                    let result = self.index.add_scopes(scopes);
                    let _ = respond.send(result);
                }
                WriteCommand::UpdateSymbolMetricsBatch { metrics, respond } => {
                    let result = self.index.update_symbol_metrics_batch(metrics);
                    let _ = respond.send(result);
                }
                WriteCommand::AddFileTags {
                    file_path,
                    tags,
                    respond,
                } => {
                    let result = self.index.add_file_tags(&file_path, &tags);
                    let _ = respond.send(result);
                }
                WriteCommand::AddDocDigestsBatch { digests, respond } => {
                    let result = self.index.add_doc_digests_batch(&digests);
                    let _ = respond.send(result);
                }
                WriteCommand::AddConfigDigest { digest, respond } => {
                    let result = self.index.add_config_digest(&digest);
                    let _ = respond.send(result);
                }
                WriteCommand::Clear { respond } => {
                    let result = self.index.clear();
                    let _ = respond.send(result);
                }
                WriteCommand::Shutdown => {
                    tracing::debug!("WriteQueue worker shutting down");
                    break;
                }
            }
        }

        tracing::debug!("WriteQueue worker stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{Location, SymbolKind};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    /// Helper to create test symbols
    fn create_symbol(name: &str, file: &str) -> Symbol {
        Symbol::new(
            name,
            SymbolKind::Function,
            Location::new(file, 1, 0, 10, 0),
            "rust",
        )
    }

    #[tokio::test]
    async fn test_write_queue_basic() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Arc::new(SqliteIndex::new(&db_path).unwrap());
        let queue = WriteQueueHandle::new(index.clone());

        // Test basic write operation
        let symbol = create_symbol("test_function", "/test/file.rs");

        queue.add_symbols(vec![symbol]).await.unwrap();

        // Verify the symbol was added
        let found = index.find_definition("test_function").unwrap();
        assert_eq!(found.len(), 1);

        queue.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_write_queue_concurrent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Arc::new(SqliteIndex::new(&db_path).unwrap());
        let queue = WriteQueueHandle::new(index.clone());

        let counter = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        // Spawn 10 concurrent write tasks
        for i in 0..10 {
            let q = queue.clone();
            let c = counter.clone();
            handles.push(tokio::spawn(async move {
                let symbol = Symbol::new(
                    &format!("func_{}", i),
                    SymbolKind::Function,
                    Location::new(&format!("/test/file_{}.rs", i), 1, 0, 10, 0),
                    "rust",
                );
                q.add_symbols(vec![symbol]).await.unwrap();
                c.fetch_add(1, Ordering::SeqCst);
            }));
        }

        // Wait for all writes to complete
        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 10);

        // Verify all symbols were added
        let stats = index.get_stats().unwrap();
        assert_eq!(stats.total_symbols, 10);

        queue.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_write_queue_remove_file() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Arc::new(SqliteIndex::new(&db_path).unwrap());
        let queue = WriteQueueHandle::new(index.clone());

        // Add a symbol
        let symbol = create_symbol("to_remove", "/test/file.rs");

        queue.add_symbols(vec![symbol]).await.unwrap();

        // Remove the file
        queue.remove_file("/test/file.rs".into()).await.unwrap();

        // Verify the symbol was removed
        let found = index.find_definition("to_remove").unwrap();
        assert!(found.is_empty());

        queue.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_write_queue_shutdown() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Arc::new(SqliteIndex::new(&db_path).unwrap());
        let queue = WriteQueueHandle::new(index);

        assert!(queue.is_active());

        queue.shutdown().await.unwrap();

        // Give the worker time to shut down
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // After shutdown, writes should fail
        let result = queue
            .add_symbols(vec![create_symbol("should_fail", "/test/file.rs")])
            .await;

        assert!(result.is_err());
    }
}
