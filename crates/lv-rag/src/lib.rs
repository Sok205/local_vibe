//! Retrieval-augmented generation primitives for LocalVibe: file scanner,
//! parsers (text, PDF, EPUB, code), chunkers, LanceDB vector store, and
//! reciprocal-rank-fusion hybrid search.

pub mod chunker;
pub mod code_graph;
pub mod hasher;
pub mod indexer;
pub mod parsers;
pub mod query;
pub mod rrf;
pub mod scanner;
pub mod store;
