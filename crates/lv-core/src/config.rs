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
    #[serde(default)]
    pub embedding: Option<ModelConfig>,
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
    #[serde(default)]
    pub db_root: Option<PathBuf>,
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
fn default_backend() -> String { "fastembed".into() }
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
            embedding: None,
            cloud: None,
        }
    }
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            db_dir: default_db_dir(),
            db_root: None,
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
        for path in Self::candidate_paths() {
            if path.exists() && let Ok(config) = Self::load(&path) {
                return config;
            }
        }
        Self::default()
    }

    /// Returns the path the active `discover()` call would have loaded from,
    /// or `None` if no candidate exists / parses. Used by the status snapshot.
    pub fn discover_path() -> Option<PathBuf> {
        Self::candidate_paths()
            .into_iter()
            .find(|p| p.exists() && Self::load(p).is_ok())
    }

    fn candidate_paths() -> Vec<PathBuf> {
        vec![
            PathBuf::from("local-vibe.toml"),
            dirs::config_dir().map(|d| d.join("local-vibe/config.toml")).unwrap_or_default(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rag_config_parses_db_root() {
        let toml_src = r#"
            [rag]
            db_root = "/tmp/xyz/dbs"
        "#;
        let cfg: Config = toml::from_str(toml_src).unwrap();
        assert_eq!(
            cfg.rag.db_root.as_deref(),
            Some(std::path::Path::new("/tmp/xyz/dbs"))
        );
    }

    #[test]
    fn rag_config_db_root_defaults_to_none() {
        let cfg = Config::default();
        assert!(cfg.rag.db_root.is_none());
    }
}
