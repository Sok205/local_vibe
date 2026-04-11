use crate::error::VibeError;
use crate::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub models: ModelsConfig,
    #[serde(default)]
    pub rag: RagConfig,
    #[serde(default)]
    pub code_graph: CodeGraphConfig,
    #[serde(default)]
    pub tui: TuiConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsConfig {
    #[serde(default = "default_fast_model")]
    pub fast: ModelConfig,
    #[serde(default = "default_medium_model")]
    pub medium: ModelConfig,
    #[serde(default = "default_strong_model")]
    pub strong: ModelConfig,
    #[serde(default = "default_embedding_model")]
    pub embedding: ModelConfig,
    pub cloud: Option<CloudConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_quantization")]
    pub quantization: String,
    pub model_path: Option<PathBuf>,
    pub tokenizer_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloudConfig {
    pub provider: String,
    pub model: String,
    pub api_key_env: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RagConfig {
    #[serde(default = "default_db_dir")]
    pub db_dir: PathBuf,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    #[serde(default = "default_code_chunk_strategy")]
    pub code_chunk_strategy: String,
    #[serde(default = "default_retrieval_limit")]
    pub retrieval_limit: usize,
    #[serde(default = "default_retrieval_threshold")]
    pub retrieval_threshold: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodeGraphConfig {
    #[serde(default = "default_languages")]
    pub languages: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_index: bool,
    #[serde(default = "default_graph_store")]
    pub store: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_true")]
    pub vim_keys: bool,
    #[serde(default = "default_true")]
    pub context_panel: bool,
}

fn default_fast_model() -> ModelConfig {
    ModelConfig { name: "gemma-4-e2b-it".into(), backend: "mlx-lm".into(), quantization: "4bit".into(), model_path: None, tokenizer_path: None }
}
fn default_medium_model() -> ModelConfig {
    ModelConfig { name: "gemma-4-26b-a4b-it".into(), backend: "mlx-lm".into(), quantization: "4bit".into(), model_path: None, tokenizer_path: None }
}
fn default_strong_model() -> ModelConfig {
    ModelConfig { name: "gemma-4-31b-it".into(), backend: "mlx-lm".into(), quantization: "4bit".into(), model_path: None, tokenizer_path: None }
}
fn default_embedding_model() -> ModelConfig {
    ModelConfig { name: "nomic-embed-text".into(), backend: "mlx-lm".into(), quantization: "fp16".into(), model_path: None, tokenizer_path: None }
}
fn default_backend() -> String { "mlx-lm".into() }
fn default_quantization() -> String { "4bit".into() }
fn default_db_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("local-vibe/db")
}
fn default_chunk_size() -> usize { 1500 }
fn default_code_chunk_strategy() -> String { "ast".into() }
fn default_retrieval_limit() -> usize { 5 }
fn default_retrieval_threshold() -> f32 { 0.5 }
fn default_languages() -> Vec<String> { vec!["rust".into(), "typescript".into(), "python".into()] }
fn default_true() -> bool { true }
fn default_graph_store() -> String { "memory".into() }

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            fast: default_fast_model(),
            medium: default_medium_model(),
            strong: default_strong_model(),
            embedding: default_embedding_model(),
            cloud: None,
        }
    }
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            db_dir: default_db_dir(),
            chunk_size: default_chunk_size(),
            code_chunk_strategy: default_code_chunk_strategy(),
            retrieval_limit: default_retrieval_limit(),
            retrieval_threshold: default_retrieval_threshold(),
        }
    }
}

impl Default for CodeGraphConfig {
    fn default() -> Self {
        Self { languages: default_languages(), auto_index: true, store: default_graph_store() }
    }
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self { vim_keys: true, context_panel: true }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| VibeError::Config(format!("failed to read {}: {e}", path.display())))?;
        toml::from_str(&content)
            .map_err(|e| VibeError::Config(format!("failed to parse {}: {e}", path.display())))
    }

    pub fn discover() -> Self {
        let candidates = [
            PathBuf::from("local-vibe.toml"),
            dirs::config_dir().map(|d| d.join("local-vibe/config.toml")).unwrap_or_default(),
        ];
        for path in &candidates {
            if path.exists() && let Ok(config) = Self::load(path) {
                return config;
            }
        }
        Self::default()
    }
}
