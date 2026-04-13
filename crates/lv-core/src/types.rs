use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::pin::Pin;
use futures::Stream;
use uuid::Uuid;

// --- Inference types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub stream: bool,
    /// Optional session identifier. Backends may keep the KV cache warm across
    /// calls sharing the same `session_id` and clear it when it changes.
    pub session_id: Option<Uuid>,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            temperature: 0.7,
            max_tokens: 4096,
            stream: true,
            session_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionChunk {
    pub delta: String,
    pub finished: bool,
}

pub type CompletionStream = Pin<Box<dyn Stream<Item = crate::Result<CompletionChunk>> + Send>>;

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub tier: ModelTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelTier {
    Fast,
    Medium,
    Strong,
    Cloud,
}

#[derive(Debug, Clone)]
pub struct BackendHealth {
    pub available: bool,
    pub model_loaded: Option<String>,
}

// --- RAG types ---

#[derive(Debug, Clone)]
pub struct Document {
    pub text: String,
    pub embedding: Vec<f32>,
    pub file_path: String,
    pub file_name: String,
    pub file_hash: String,
    pub chunk_index: u32,
    pub language: Option<String>,
    pub symbol_context: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub text: String,
    pub score: f32,
    pub file_path: String,
    pub file_name: String,
    pub chunk_index: u32,
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    pub language: Option<String>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoreStats {
    pub total_chunks: usize,
    pub unique_files: usize,
}

#[derive(Debug, Clone)]
pub struct FileSummary {
    pub file_path: String,
    pub language: Option<String>,
    pub chunk_count: usize,
}

// --- Code graph types ---

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct SymbolId {
    pub file_path: PathBuf,
    pub name: String,
    pub kind: SymbolKind,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize)]
pub enum SymbolKind {
    Function,
    Struct,
    Trait,
    Impl,
    Enum,
    Const,
    Module,
    Import,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub id: SymbolId,
    pub span: Span,
    pub signature: String,
}

#[derive(Debug, Clone)]
pub struct Span {
    pub start_line: usize,
    pub end_line: usize,
    pub start_col: usize,
    pub end_col: usize,
}

#[derive(Debug, Clone)]
pub struct Location {
    pub file_path: PathBuf,
    pub span: Span,
}

// --- Indexing types ---

#[derive(Debug, Clone)]
pub enum IndexProgress {
    Scanning,
    Indexing { done: usize, total: usize, current: String },
    Complete { indexed: usize, skipped: usize, failed: usize },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub text: String,
    pub file_path: PathBuf,
    pub file_name: String,
    pub extension: String,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub text: String,
    pub start_offset: usize,
    pub end_offset: usize,
}
