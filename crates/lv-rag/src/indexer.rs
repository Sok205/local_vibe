use std::path::Path;
use std::sync::Arc;

use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;

use lv_core::traits::{Chunker, EmbeddingBackend, Parser, VectorStore};
use lv_core::types::{Document, IndexProgress};
use lv_core::{Result, VibeError};

use crate::hasher;
use crate::scanner::{scan_directory, ScannedFile};

pub struct IndexManager {
    parsers: Vec<Box<dyn Parser>>,
    chunker: Box<dyn Chunker>,
    embedder: Arc<dyn EmbeddingBackend>,
    store: Arc<dyn VectorStore>,
    concurrency: usize,
}

impl IndexManager {
    pub fn new(
        parsers: Vec<Box<dyn Parser>>,
        chunker: Box<dyn Chunker>,
        embedder: Arc<dyn EmbeddingBackend>,
        store: Arc<dyn VectorStore>,
        concurrency: usize,
    ) -> Self {
        Self {
            parsers,
            chunker,
            embedder,
            store,
            concurrency,
        }
    }

    pub async fn index(
        self,
        docs_dir: &Path,
    ) -> Result<(
        mpsc::Receiver<IndexProgress>,
        tokio::task::JoinHandle<Result<()>>,
        CancellationToken,
    )> {
        let (tx, rx) = mpsc::channel(128);
        let cancel = CancellationToken::new();

        let supported: Vec<&str> = self
            .parsers
            .iter()
            .flat_map(|p| p.supported_extensions().iter().copied())
            .collect();

        let _ = tx.send(IndexProgress::Scanning).await;
        let files = scan_directory(docs_dir, &supported)?;

        let parsers = Arc::new(self.parsers);
        let chunker: Arc<dyn Chunker> = Arc::from(self.chunker);
        let embedder = self.embedder;
        let store = self.store;
        let semaphore = Arc::new(Semaphore::new(self.concurrency));
        let total = files.len();
        let cancel_clone = cancel.clone();

        let handle = tokio::spawn(async move {
            let mut indexed = 0usize;
            let mut failed = 0usize;
            let mut skipped = 0usize;
            let mut done = 0usize;

            let mut handles = Vec::with_capacity(total);

            for file in files {
                if cancel_clone.is_cancelled() {
                    break;
                }

                let permit = semaphore.clone().acquire_owned().await.unwrap();
                let parsers = parsers.clone();
                let chunker = chunker.clone();
                let embedder = embedder.clone();
                let store = store.clone();
                let cancel = cancel_clone.clone();

                let h = tokio::spawn(async move {
                    if cancel.is_cancelled() {
                        drop(permit);
                        return (file, Ok(FileOutcome::Skipped));
                    }
                    let result =
                        index_single_file(&file, &parsers, &*chunker, &*embedder, &*store).await;
                    drop(permit);
                    (file, result)
                });
                handles.push(h);
            }

            for h in handles {
                done += 1;
                match h.await {
                    Ok((file, Ok(FileOutcome::Indexed))) => {
                        indexed += 1;
                        let _ = tx
                            .send(IndexProgress::Indexing {
                                done,
                                total,
                                current: file.name,
                            })
                            .await;
                    }
                    Ok((file, Ok(FileOutcome::Skipped))) => {
                        skipped += 1;
                        let _ = tx
                            .send(IndexProgress::Indexing {
                                done,
                                total,
                                current: file.name,
                            })
                            .await;
                    }
                    Ok((file, Err(e))) => {
                        failed += 1;
                        tracing::warn!("Failed to index {}: {e}", file.name);
                        let _ = tx
                            .send(IndexProgress::Indexing {
                                done,
                                total,
                                current: file.name,
                            })
                            .await;
                    }
                    Err(join_err) => {
                        failed += 1;
                        tracing::error!("Task panicked: {join_err}");
                        let _ = tx
                            .send(IndexProgress::Indexing {
                                done,
                                total,
                                current: "<panicked task>".to_string(),
                            })
                            .await;
                    }
                }
            }

            let _ = tx
                .send(IndexProgress::Complete {
                    indexed,
                    skipped,
                    failed,
                })
                .await;
            Ok(())
        });

        Ok((rx, handle, cancel))
    }
}

enum FileOutcome {
    Indexed,
    Skipped,
}

async fn index_single_file(
    file: &ScannedFile,
    parsers: &[Box<dyn Parser>],
    chunker: &dyn Chunker,
    embedder: &dyn EmbeddingBackend,
    store: &dyn VectorStore,
) -> Result<FileOutcome> {
    let file_hash = hasher::hash_file(&file.path)?;

    if store.has_file(&file_hash).await? {
        return Ok(FileOutcome::Skipped);
    }

    let doc = crate::parsers::parse_document(&file.path, parsers)?;

    let chunks = chunker.chunk(&doc.text, Some(&file.path));
    if chunks.is_empty() {
        return Err(VibeError::Parse {
            path: file.path.clone(),
            reason: "no chunks produced".to_string(),
        });
    }

    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    let vectors = embedder.embed(&texts).await?;

    let file_path_str = file.path.to_string_lossy().into_owned();
    let language = language_from_path(&file.path);
    let documents: Vec<Document> = chunks
        .iter()
        .zip(vectors.into_iter())
        .enumerate()
        .map(|(i, (chunk, embedding))| Document {
            text: chunk.text.clone(),
            embedding,
            file_path: file_path_str.clone(),
            file_name: file.name.clone(),
            file_hash: file_hash.clone(),
            chunk_index: i as u32,
            language: language.clone(),
            symbol_context: None,
        })
        .collect();

    store.add_documents(&documents).await?;
    Ok(FileOutcome::Indexed)
}

fn language_from_path(path: &std::path::Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let lang = match ext.as_str() {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" | "pyi" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "sh" | "bash" | "zsh" => "shell",
        "md" | "markdown" => "markdown",
        "html" | "htm" => "html",
        "css" | "scss" | "sass" | "less" => "css",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "sql" => "sql",
        "pdf" => "pdf",
        "epub" => "epub",
        "txt" => "text",
        _ => return None,
    };
    Some(lang.to_string())
}

#[cfg(test)]
mod language_tests {
    use super::language_from_path;
    use std::path::Path;

    #[test]
    fn maps_common_code_extensions() {
        assert_eq!(language_from_path(Path::new("a.rs")).as_deref(), Some("rust"));
        assert_eq!(language_from_path(Path::new("a.tsx")).as_deref(), Some("typescript"));
        assert_eq!(language_from_path(Path::new("a.PY")).as_deref(), Some("python"));
        assert_eq!(language_from_path(Path::new("a.md")).as_deref(), Some("markdown"));
    }

    #[test]
    fn returns_none_for_unknown_extension() {
        assert_eq!(language_from_path(Path::new("a.xyz")), None);
        assert_eq!(language_from_path(Path::new("noext")), None);
    }
}
