//! Integration tests for local-vibe
//!
//! Tests marked #[ignore] require a running mlx-lm server on port 8080.
//! Run with: cargo test -- --ignored

use lv_core::types::*;
use lv_core::traits::CodeGraph;
use lv_rag::code_graph::TreeSitterGraph;
use lv_rag::chunker::{OverlappingChunker, AstChunker};
use lv_core::traits::Chunker;
use std::path::Path;

#[test]
fn test_code_graph_indexes_rust() {
    let mut graph = TreeSitterGraph::new(&["rust".to_string()]);
    let code = r#"
fn hello() -> String {
    "world".to_string()
}

struct Config {
    name: String,
}

impl Config {
    fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

trait Greeter {
    fn greet(&self) -> String;
}
"#;
    graph.index_file(Path::new("test.rs"), code).unwrap();
    let symbols = graph.symbols(Path::new("test.rs"));

    // Should find: fn hello, struct Config, impl Config, trait Greeter
    assert!(symbols.len() >= 3, "expected >= 3 symbols, got {}", symbols.len());

    let names: Vec<&str> = symbols.iter().map(|s| s.id.name.as_str()).collect();
    assert!(names.contains(&"hello"), "missing fn hello");
    assert!(names.contains(&"Config"), "missing struct Config");
    assert!(names.contains(&"Greeter"), "missing trait Greeter");
}

#[test]
fn test_code_graph_repo_map() {
    let mut graph = TreeSitterGraph::new(&["rust".to_string()]);
    graph.index_file(Path::new("src/main.rs"), "fn main() {}").unwrap();
    graph.index_file(Path::new("src/lib.rs"), "pub fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();

    let map = graph.repo_map(Path::new(""));
    assert!(map.contains("main"), "repo map should contain main");
    assert!(map.contains("add"), "repo map should contain add");
}

#[test]
fn test_ast_chunker_splits_functions() {
    let chunker = AstChunker::new();
    let code = r#"fn foo() -> i32 { 42 }

fn bar() -> String { "hello".to_string() }

struct Baz { x: i32 }
"#;
    let chunks = chunker.chunk(code, Some(Path::new("test.rs")));
    assert!(chunks.len() >= 2, "expected >= 2 chunks, got {}", chunks.len());
}

#[test]
fn test_overlapping_chunker() {
    let chunker = OverlappingChunker::new(10, 2);
    let text = "one two three four five six seven eight nine ten eleven twelve";
    let chunks = chunker.chunk(text, None);
    assert!(chunks.len() >= 2, "expected >= 2 chunks from overlapping chunker");
    // Verify overlap: last words of chunk 0 should appear in chunk 1
    if chunks.len() >= 2 {
        assert!(chunks[1].text.contains("ten"), "expected overlap in chunks");
    }
}

#[test]
fn test_config_default() {
    let config = lv_core::Config::default();
    assert_eq!(config.models.fast.name, "gemma-4-e2b-it");
    assert_eq!(config.models.medium.name, "gemma-4-26b-a4b-it");
    assert_eq!(config.models.strong.name, "gemma-4-31b-it");
    assert_eq!(config.models.embedding.name, "nomic-embed-text");
}

#[test]
fn test_router_empty() {
    let router = lv_router::EscalatingRouter::new();
    assert!(router.backend_at(ModelTier::Fast).is_none());
    assert!(router.list_models().is_empty());
}

// --- Tests requiring mlx-lm server ---

#[tokio::test]
#[ignore]
async fn test_mlx_backend_health() {
    use lv_inference::mlx_lm::MlxLmBackend;
    use lv_core::traits::InferenceBackend;

    let backend = MlxLmBackend::connect("gemma-4-26b-a4b-it", 8080, ModelTier::Medium);
    let health = backend.health().await;
    assert!(health.available, "mlx-lm server not running on port 8080");
}

#[tokio::test]
#[ignore]
async fn test_mlx_backend_embed() {
    use lv_inference::mlx_lm::MlxLmBackend;
    use lv_core::traits::InferenceBackend;

    let backend = MlxLmBackend::connect("nomic-embed-text", 8080, ModelTier::Fast);
    let embeddings = backend.embed(&["test query about rust code"]).await.unwrap();
    assert_eq!(embeddings.len(), 1);
    assert!(!embeddings[0].is_empty(), "embedding should not be empty");
}
