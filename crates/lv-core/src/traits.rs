use crate::config::Config;
use crate::status::RuntimeStatus;
use crate::types::*;
use crate::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionStream>;
    fn model_info(&self) -> ModelInfo;
    async fn health(&self) -> BackendHealth;
}

/// Narrow capability surface that every consumer (CLI status, TUI status overlay,
/// MCP tools) uses to reach the running application. Implemented by AppContext in
/// lv-cli; kept here so MCP can depend on it without a circular crate dependency.
#[async_trait]
pub trait AppHost: Send + Sync {
    fn config(&self) -> &Config;
    async fn embedding(&self) -> anyhow::Result<Option<Arc<dyn EmbeddingBackend>>>;
    async fn store_named(&self, name: &str) -> anyhow::Result<Arc<dyn VectorStore>>;
    async fn open_store_readonly(&self, name: &str) -> anyhow::Result<Arc<dyn VectorStore>>;
    async fn list_dbs(&self) -> anyhow::Result<Vec<String>>;
    async fn current_db(&self) -> String;
    async fn runtime_status(&self) -> RuntimeStatus;

    async fn load_model(&self, tier: ModelTier) -> anyhow::Result<()>;
    async fn unload_model(&self, tier: ModelTier) -> anyhow::Result<()>;
    async fn set_active_tier(&self, tier: ModelTier) -> anyhow::Result<()>;
    async fn warm_tiers(&self) -> Vec<ModelTier>;
    async fn active_tier(&self) -> ModelTier;
    async fn is_embedding_warm(&self) -> bool;
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
