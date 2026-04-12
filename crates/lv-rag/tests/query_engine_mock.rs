use async_trait::async_trait;
use std::sync::Arc;

use lv_core::traits::{EmbeddingBackend, VectorStore};
use lv_core::types::{Document, SearchFilter, SearchResult, StoreStats};
use lv_core::Result;
use lv_rag::query::QueryEngine;

struct StubEmbedder;

#[async_trait]
impl EmbeddingBackend for StubEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![1.0, 0.0, 0.0]).collect())
    }
    fn dim(&self) -> usize { 3 }
    fn model_name(&self) -> &str { "stub" }
}

struct StubStore;

#[async_trait]
impl VectorStore for StubStore {
    async fn add_documents(&self, _: &[Document]) -> Result<()> { Ok(()) }
    async fn search(&self, _q: &[f32], _limit: usize, _thr: f32, _f: &SearchFilter) -> Result<Vec<SearchResult>> {
        Ok(vec![SearchResult {
            text: "hit".into(), score: 0.9,
            file_path: "f".into(), file_name: "f".into(), chunk_index: 0,
        }])
    }
    async fn delete_by_hash(&self, _: &str) -> Result<()> { Ok(()) }
    async fn has_file(&self, _: &str) -> Result<bool> { Ok(false) }
    async fn stats(&self) -> Result<StoreStats> { Ok(StoreStats { total_chunks: 0, unique_files: 0 }) }
}

#[tokio::test]
async fn query_engine_uses_embedding_backend() {
    let engine = QueryEngine::new(Arc::new(StubEmbedder), Arc::new(StubStore));
    let out = engine.search("hello", 5, 0.0, &SearchFilter::default()).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "hit");
}
