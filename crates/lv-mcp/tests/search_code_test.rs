//! Integration test for the MCP `search_code` path.
//!
//! Tests the `AppHost` trait wiring that `VibeMcpServer::search_code`
//! consumes: a mock host exposes a stub embedder and stub vector store,
//! and the test drives the same `QueryEngine` flow the handler uses.
//!
//! A full wire-protocol test (stdio transport + JSON-RPC `CallTool`) would
//! require an rmcp test harness that doesn't exist on this version; this
//! test covers the host-trait surface end-to-end, which is the only place
//! the handler does meaningful work outside of MCP plumbing.

use std::sync::Arc;

use async_trait::async_trait;
use lv_core::Config;
use lv_core::Result;
use lv_core::status::RuntimeStatus;
use lv_core::traits::{AppHost, EmbeddingBackend, VectorStore};
use lv_core::types::{Document, FileSummary, ModelTier, SearchFilter, SearchResult, StoreStats};
use lv_rag::query::QueryEngine;

struct StubEmbedder;

#[async_trait]
impl EmbeddingBackend for StubEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![1.0, 0.0, 0.0]).collect())
    }
    fn dim(&self) -> usize {
        3
    }
    fn model_name(&self) -> &str {
        "stub"
    }
}

struct StubStore {
    name: String,
}

#[async_trait]
impl VectorStore for StubStore {
    async fn add_documents(&self, _: &[Document]) -> Result<()> {
        Ok(())
    }
    async fn search(
        &self,
        _q: &[f32],
        _limit: usize,
        _thr: f32,
        _f: &SearchFilter,
    ) -> Result<Vec<SearchResult>> {
        Ok(vec![SearchResult {
            text: format!("hit from {}", self.name),
            score: 0.9,
            file_path: "src/main.rs".into(),
            file_name: "main.rs".into(),
            chunk_index: 0,
        }])
    }
    async fn delete_by_hash(&self, _: &str) -> Result<()> {
        Ok(())
    }
    async fn has_file(&self, _: &str) -> Result<bool> {
        Ok(false)
    }
    async fn stats(&self) -> Result<StoreStats> {
        Ok(StoreStats {
            total_chunks: 1,
            unique_files: 1,
        })
    }
    async fn list_files(&self, _: usize) -> Result<Vec<FileSummary>> {
        Ok(Vec::new())
    }
}

struct MockAppHost {
    config: Config,
}

#[async_trait]
impl AppHost for MockAppHost {
    fn config(&self) -> &Config {
        &self.config
    }
    async fn embedding(&self) -> anyhow::Result<Option<Arc<dyn EmbeddingBackend>>> {
        Ok(Some(Arc::new(StubEmbedder)))
    }
    async fn store_named(&self, name: &str) -> anyhow::Result<Arc<dyn VectorStore>> {
        Ok(Arc::new(StubStore { name: name.into() }))
    }
    async fn open_store_readonly(&self, name: &str) -> anyhow::Result<Arc<dyn VectorStore>> {
        self.store_named(name).await
    }
    async fn list_dbs(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec!["default".into()])
    }
    async fn current_db(&self) -> String {
        "default".into()
    }
    async fn runtime_status(&self) -> RuntimeStatus {
        RuntimeStatus::default()
    }
    async fn load_model(&self, _: ModelTier) -> anyhow::Result<()> {
        Ok(())
    }
    async fn unload_model(&self, _: ModelTier) -> anyhow::Result<()> {
        Ok(())
    }
    async fn set_active_tier(&self, _: ModelTier) -> anyhow::Result<()> {
        Ok(())
    }
    async fn warm_tiers(&self) -> Vec<ModelTier> {
        Vec::new()
    }
    async fn active_tier(&self) -> ModelTier {
        ModelTier::Medium
    }
    async fn is_embedding_warm(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn host_trait_drives_search_flow() {
    // Exercise the exact pipeline `search_code` runs: pull embedder + store
    // off the host, build a QueryEngine, and search.
    let host: Arc<dyn AppHost> = Arc::new(MockAppHost {
        config: Config::default(),
    });

    let embedder = host
        .embedding()
        .await
        .unwrap()
        .expect("mock host returns an embedder");
    let store = host.store_named("default").await.unwrap();

    let engine = QueryEngine::new(embedder, store);
    let results = engine
        .search("anything", 5, 0.0, &SearchFilter::default())
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].text, "hit from default");
    assert_eq!(results[0].file_path, "src/main.rs");
}

#[tokio::test]
async fn host_trait_routes_db_parameter() {
    // Mimic the `db` parameter path: search_code dispatches to
    // `open_store_readonly(name)` when a db is provided.
    let host: Arc<dyn AppHost> = Arc::new(MockAppHost {
        config: Config::default(),
    });

    let store = host.open_store_readonly("rocksdb").await.unwrap();
    let embedder = host.embedding().await.unwrap().unwrap();

    let engine = QueryEngine::new(embedder, store);
    let results = engine
        .search("compaction", 5, 0.0, &SearchFilter::default())
        .await
        .unwrap();

    assert_eq!(results[0].text, "hit from rocksdb");
}
