use crate::types::*;
use crate::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionStream>;
    fn model_info(&self) -> ModelInfo;
    async fn health(&self) -> BackendHealth;
}

#[async_trait]
pub trait EmbeddingBackend: Send + Sync {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dim(&self) -> usize;
    fn model_name(&self) -> &str;
}

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn add_documents(&self, docs: &[Document]) -> Result<()>;
    async fn search(&self, query: &[f32], limit: usize, threshold: f32, filter: &SearchFilter) -> Result<Vec<SearchResult>>;
    async fn delete_by_hash(&self, file_hash: &str) -> Result<()>;
    async fn has_file(&self, file_hash: &str) -> Result<bool>;
    async fn stats(&self) -> Result<StoreStats>;
    async fn list_files(&self, limit: usize) -> Result<Vec<FileSummary>>;
}

pub trait Parser: Send + Sync {
    fn supported_extensions(&self) -> &[&str];
    fn parse(&self, path: &Path) -> Result<ParsedDocument>;
}

pub trait Chunker: Send + Sync {
    fn chunk(&self, text: &str, file_path: Option<&Path>) -> Vec<Chunk>;
}

pub trait CodeGraph: Send + Sync {
    fn index_file(&mut self, path: &Path, content: &str) -> Result<()>;
    fn symbols(&self, path: &Path) -> Vec<Symbol>;
    fn references(&self, symbol: &SymbolId) -> Vec<Location>;
    fn dependents(&self, path: &Path) -> Vec<PathBuf>;
    fn repo_map(&self, root: &Path) -> String;
}
