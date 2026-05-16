//! MCP (Model Context Protocol) server exposing LocalVibe semantic search
//! and inference to AI assistants like Claude Code, Cursor, and Zed.

pub mod server;
pub use server::{VibeMcpServer, run_stdio};
