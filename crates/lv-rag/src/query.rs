use std::sync::Arc;

use lv_core::traits::{EmbeddingBackend, VectorStore};
use lv_core::types::{SearchFilter, SearchResult};
use lv_core::{Result, VibeError};

const PROMPT_TEMPLATE: &str = r#"You are a helpful assistant. Use the following context from indexed documents to answer the user's question. If the context doesn't contain relevant information, say so.

Context:
{{rag_context}}

User Question:
{{user_query}}"#;

pub struct QueryEngine {
    embedder: Arc<dyn EmbeddingBackend>,
    store: Arc<dyn VectorStore>,
}

impl QueryEngine {
    pub fn new(embedder: Arc<dyn EmbeddingBackend>, store: Arc<dyn VectorStore>) -> Self {
        Self { embedder, store }
    }

    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        threshold: f32,
        filter: &SearchFilter,
    ) -> Result<Vec<SearchResult>> {
        let vectors = self.embedder.embed(&[query]).await?;
        let query_vector = vectors
            .into_iter()
            .next()
            .ok_or_else(|| VibeError::Embedding("No embedding returned".to_string()))?;

        self.store
            .search(&query_vector, limit, threshold, filter)
            .await
    }

    pub async fn query(
        &self,
        user_query: &str,
        limit: usize,
        threshold: f32,
        filter: &SearchFilter,
    ) -> Result<String> {
        let results = self.search(user_query, limit, threshold, filter).await?;

        if results.is_empty() {
            return Ok(format!(
                "No relevant content found in indexed documents.\n\nUser Query:\n\n{user_query}"
            ));
        }

        let mut context = String::with_capacity(results.len() * 256);
        for (i, result) in results.iter().enumerate() {
            let file_name = result
                .file_path
                .rsplit('/')
                .next()
                .unwrap_or(&result.file_path);
            context.push_str(&format!(
                "\nCitation {} (from {}, score: {:.3}): \"{}\"\n\n",
                i + 1,
                file_name,
                result.score,
                result.text
            ));
        }

        let prompt = PROMPT_TEMPLATE
            .replace("{{rag_context}}", context.trim())
            .replace("{{user_query}}", user_query);

        Ok(prompt)
    }
}
