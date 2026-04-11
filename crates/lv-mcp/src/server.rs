use std::path::Path;
use std::sync::Arc;

use lv_core::traits::{InferenceBackend, VectorStore};
use lv_core::types::{IndexProgress, SearchFilter};
use lv_rag::chunker::OverlappingChunker;
use lv_rag::indexer::IndexManager;
use lv_rag::parsers::{epub::EpubParser, html::HtmlParser, pdf::PdfParser, text::TextParser};
use lv_rag::query::QueryEngine;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchCodeParams {
    /// The search query string
    pub query: String,
    /// Maximum number of results to return (default: 5)
    pub limit: Option<u32>,
    /// Minimum similarity score threshold 0.0-1.0 (default: 0.3)
    pub threshold: Option<f32>,
    /// Filter by language (e.g. "rust", "typescript")
    pub language: Option<String>,
    /// Filter by file path (exact match)
    pub file_path: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct IndexDirectoryParams {
    /// Absolute path to the directory to index
    pub path: String,
    /// Chunk size in words (default: 200)
    pub chunk_size: Option<usize>,
    /// Chunk overlap in words (default: 40)
    pub chunk_overlap: Option<usize>,
    /// Concurrency limit (default: 4)
    pub concurrency: Option<usize>,
}

#[derive(Clone)]
pub struct VibeMcpServer {
    backend: Arc<dyn InferenceBackend>,
    store: Arc<dyn VectorStore>,
    tool_router: ToolRouter<Self>,
}

impl VibeMcpServer {
    pub fn new(backend: Arc<dyn InferenceBackend>, store: Arc<dyn VectorStore>) -> Self {
        Self {
            backend,
            store,
            tool_router: Self::tool_router(),
        }
    }

    fn build_parsers() -> Vec<Box<dyn lv_core::traits::Parser>> {
        vec![
            Box::new(TextParser),
            Box::new(PdfParser),
            Box::new(HtmlParser),
            Box::new(EpubParser),
        ]
    }
}

#[tool_router]
impl VibeMcpServer {
    #[tool(description = "Search indexed code and documents using semantic similarity. Returns relevant chunks with file citations and similarity scores.")]
    async fn search_code(
        &self,
        Parameters(params): Parameters<SearchCodeParams>,
    ) -> Result<CallToolResult, McpError> {
        let limit = params.limit.unwrap_or(5) as usize;
        let threshold = params.threshold.unwrap_or(0.3);

        let filter = SearchFilter {
            language: params.language,
            file_path: params.file_path,
        };

        let query_engine = QueryEngine::new(self.backend.clone(), self.store.clone());
        let results = query_engine
            .search(&params.query, limit, threshold, &filter)
            .await
            .map_err(|e| McpError::internal_error(format!("Search failed: {e}"), None))?;

        if results.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No relevant code found for this query.",
            )]));
        }

        let mut output = String::new();
        for (i, r) in results.iter().enumerate() {
            output.push_str(&format!(
                "--- Result {} (score: {:.3}, file: {}) ---\n{}\n\n",
                i + 1,
                r.score,
                r.file_path,
                r.text
            ));
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Index a local directory into the vector store. Parses code and documents (Rust, TypeScript, Python, Markdown, PDF, HTML, EPUB) into chunks, embeds them, and stores for semantic search.")]
    async fn index_directory(
        &self,
        Parameters(params): Parameters<IndexDirectoryParams>,
    ) -> Result<CallToolResult, McpError> {
        let dir = Path::new(&params.path);
        if !dir.exists() {
            return Err(McpError::invalid_params(
                format!("Directory does not exist: {}", params.path),
                None,
            ));
        }
        if !dir.is_dir() {
            return Err(McpError::invalid_params(
                format!("Path is not a directory: {}", params.path),
                None,
            ));
        }

        let chunk_size = params.chunk_size.unwrap_or(200);
        let chunk_overlap = params.chunk_overlap.unwrap_or(40);
        let concurrency = params.concurrency.unwrap_or(4);

        let manager = IndexManager::new(
            Self::build_parsers(),
            Box::new(OverlappingChunker::new(chunk_size, chunk_overlap)),
            self.backend.clone(),
            self.store.clone(),
            concurrency,
        );

        let (mut rx, handle, _cancel) = manager.index(dir).await.map_err(|e| {
            McpError::internal_error(format!("Failed to start indexing: {e}"), None)
        })?;

        let mut indexed = 0usize;
        let mut skipped = 0usize;
        let mut failed = 0usize;

        while let Some(progress) = rx.recv().await {
            if let IndexProgress::Complete {
                indexed: i,
                skipped: s,
                failed: f,
            } = progress
            {
                indexed = i;
                skipped = s;
                failed = f;
            }
        }

        handle.await.map_err(|e| {
            McpError::internal_error(format!("Indexing task panicked: {e}"), None)
        })?.map_err(|e| {
            McpError::internal_error(format!("Indexing failed: {e}"), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Indexing complete: {} indexed, {} skipped, {} failed",
            indexed, skipped, failed
        ))]))
    }

    #[tool(description = "Get statistics about the indexed code store: total chunks and unique files.")]
    async fn get_stats(&self) -> Result<CallToolResult, McpError> {
        let stats = self
            .store
            .stats()
            .await
            .map_err(|e| McpError::internal_error(format!("Stats failed: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Indexed store: {} chunks from {} unique files",
            stats.total_chunks, stats.unique_files
        ))]))
    }

    #[tool(description = "Get a summary of indexed files in the vector store.")]
    async fn list_sources(&self) -> Result<CallToolResult, McpError> {
        let stats = self
            .store
            .stats()
            .await
            .map_err(|e| McpError::internal_error(format!("List sources failed: {e}"), None))?;

        if stats.total_chunks == 0 {
            return Ok(CallToolResult::success(vec![Content::text(
                "No files indexed yet.",
            )]));
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "{} unique files indexed with {} total chunks.\n\nUse search_code to query the indexed content.",
            stats.unique_files, stats.total_chunks
        ))]))
    }
}

#[tool_handler]
impl ServerHandler for VibeMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(
            ServerCapabilities::builder().enable_tools().build(),
        );
        info.instructions = Some(
            "LocalVibe MCP: semantic search over locally indexed code and documents. \
            Index directories of Rust, TypeScript, Python, Markdown, PDF, HTML, or EPUB files, \
            then search them with natural language queries."
                .into(),
        );
        info
    }
}
