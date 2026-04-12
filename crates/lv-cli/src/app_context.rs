use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{OnceCell, RwLock};

use anyhow::Context;
use lv_core::traits::{EmbeddingBackend, InferenceBackend, VectorStore};
use lv_core::types::ModelTier;
use lv_core::Config;
use lv_inference::mlx_lm::MlxLmBackend;
use lv_metal::MetalBackend;
use lv_rag::code_graph::TreeSitterGraph;
use lv_rag::store::LanceStore;

pub const DEFAULT_DB_NAME: &str = "default";

/// Lazy holder for every expensive component. Each field initializes on first
/// access; a subcommand that does not touch embedding pays no embedding cost.
pub struct AppContext {
    pub config: Config,
    inference: OnceCell<Arc<dyn InferenceBackend>>,
    embedding: OnceCell<Option<Arc<dyn EmbeddingBackend>>>,
    embed_dim: OnceCell<Option<usize>>,
    stores: RwLock<HashMap<String, Arc<dyn VectorStore>>>,
    current_db: RwLock<String>,
    code_graph: OnceCell<Arc<RwLock<TreeSitterGraph>>>,
}

#[allow(dead_code)]
impl AppContext {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            inference: OnceCell::new(),
            embedding: OnceCell::new(),
            embed_dim: OnceCell::new(),
            stores: RwLock::new(HashMap::new()),
            current_db: RwLock::new(DEFAULT_DB_NAME.to_string()),
            code_graph: OnceCell::new(),
        }
    }

    pub async fn inference(&self) -> anyhow::Result<Arc<dyn InferenceBackend>> {
        self.inference
            .get_or_try_init(|| async { build_inference(&self.config).await })
            .await
            .cloned()
    }

    /// Returns `None` if no embedding model is configured — RAG is disabled.
    pub async fn embedding(&self) -> anyhow::Result<Option<Arc<dyn EmbeddingBackend>>> {
        self.embedding
            .get_or_try_init(|| async { build_embedding(&self.config).await })
            .await
            .cloned()
    }

    async fn ensure_dim(&self) -> anyhow::Result<usize> {
        let cached = self
            .embed_dim
            .get_or_try_init(|| async {
                let Some(embedder) = self.embedding().await? else {
                    return Ok::<_, anyhow::Error>(None);
                };
                let _ = embedder
                    .embed(&["dimension probe"])
                    .await
                    .context("embedding probe failed")?;
                let dim = embedder.dim();
                if dim == 0 {
                    anyhow::bail!("embedder returned empty vector");
                }
                Ok(Some(dim))
            })
            .await?;
        cached.ok_or_else(|| anyhow::anyhow!("no embedding model configured"))
    }

    fn db_path_for(&self, name: &str) -> anyhow::Result<String> {
        if name == DEFAULT_DB_NAME && self.config.rag.db_root.is_none() {
            return Ok(self.config.rag.db_dir.to_string_lossy().to_string());
        }
        let root = self.config.rag.db_root.as_ref().ok_or_else(|| {
            anyhow::anyhow!("rag.db_root is not configured; set it to enable multi-DB mode")
        })?;
        Ok(root.join(name).to_string_lossy().to_string())
    }

    pub async fn store_named(&self, name: &str) -> anyhow::Result<Arc<dyn VectorStore>> {
        {
            let guard = self.stores.read().await;
            if let Some(s) = guard.get(name) {
                return Ok(Arc::clone(s));
            }
        }
        let dim = self.ensure_dim().await?;
        let path = self.db_path_for(name)?;
        let store: Arc<dyn VectorStore> = Arc::new(
            LanceStore::new(&path, dim)
                .await
                .with_context(|| format!("failed to open LanceStore at {path}"))?,
        );
        let mut guard = self.stores.write().await;
        let entry = guard
            .entry(name.to_string())
            .or_insert_with(|| Arc::clone(&store));
        Ok(Arc::clone(entry))
    }

    pub async fn store(&self) -> anyhow::Result<Arc<dyn VectorStore>> {
        let name = self.current_db.read().await.clone();
        self.store_named(&name).await
    }

    pub async fn current_db(&self) -> String {
        self.current_db.read().await.clone()
    }

    pub async fn set_current_db(&self, name: &str) -> anyhow::Result<()> {
        let available = self.list_dbs().await.unwrap_or_default();
        if !available.is_empty() && !available.iter().any(|n| n == name) {
            anyhow::bail!("unknown DB '{name}'; available: [{}]", available.join(", "));
        }
        *self.current_db.write().await = name.to_string();
        Ok(())
    }

    pub async fn list_dbs(&self) -> anyhow::Result<Vec<String>> {
        let Some(root) = self.config.rag.db_root.as_ref() else {
            anyhow::bail!("rag.db_root is not configured");
        };
        let mut names = Vec::new();
        let read = match std::fs::read_dir(root) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(names),
            Err(e) => anyhow::bail!("failed to read db_root: {e}"),
        };
        for entry in read.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && let Some(name) = entry.file_name().to_str()
            {
                names.push(name.to_string());
            }
        }
        names.sort();
        Ok(names)
    }

    pub async fn code_graph(&self) -> Arc<RwLock<TreeSitterGraph>> {
        self.code_graph
            .get_or_init(|| async {
                Arc::new(RwLock::new(TreeSitterGraph::new(
                    &self.config.code_graph.languages,
                )))
            })
            .await
            .clone()
    }
}

async fn build_inference(config: &Config) -> anyhow::Result<Arc<dyn InferenceBackend>> {
    let m = &config.models.medium;
    let backend: Arc<dyn InferenceBackend> = match m.backend.as_str() {
        "metal" => {
            let model_path = m
                .model_path
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("model_path required for metal backend"))?;
            let tokenizer_path = m
                .tokenizer_path
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("tokenizer_path required for metal backend"))?;
            Arc::new(MetalBackend::load(
                model_path,
                tokenizer_path,
                ModelTier::Medium,
            )?)
        }
        _ => Arc::new(MlxLmBackend::connect(&m.name, 8080, ModelTier::Medium)),
    };
    Ok(backend)
}

async fn build_embedding(
    config: &Config,
) -> anyhow::Result<Option<Arc<dyn EmbeddingBackend>>> {
    let Some(ref m) = config.models.embedding else {
        return Ok(None);
    };
    let backend: Arc<dyn EmbeddingBackend> = match m.backend.as_str() {
        "fastembed" | "" => {
            let fb = lv_inference::fastembed_backend::FastEmbedBackend::new(&m.name)
                .map_err(|e| anyhow::anyhow!("fastembed init: {e}"))?;
            Arc::new(fb)
        }
        "mlx-lm" | "mlx" => Arc::new(MlxLmBackend::connect(&m.name, 8081, ModelTier::Fast)),
        other => anyhow::bail!(
            "embedding backend '{other}' is not supported; use 'fastembed' \
             or 'mlx-lm', or omit [models.embedding] to disable RAG"
        ),
    };
    Ok(Some(backend))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn cfg_with_db_root(root: &std::path::Path) -> Config {
        let mut cfg = Config::default();
        cfg.rag.db_root = Some(root.to_path_buf());
        cfg
    }

    #[tokio::test]
    async fn list_dbs_returns_subdirs_sorted() {
        let td = tempdir().unwrap();
        for name in ["zeta", "alpha", "mid"] {
            std::fs::create_dir_all(td.path().join(name)).unwrap();
        }
        std::fs::write(td.path().join("ignore.txt"), "x").unwrap();

        let ctx = AppContext::new(cfg_with_db_root(td.path()));
        let dbs = ctx.list_dbs().await.unwrap();
        assert_eq!(
            dbs,
            vec!["alpha".to_string(), "mid".to_string(), "zeta".to_string()]
        );
    }

    #[tokio::test]
    async fn list_dbs_returns_empty_when_root_missing() {
        let td = tempdir().unwrap();
        let missing = td.path().join("does-not-exist");
        let ctx = AppContext::new(cfg_with_db_root(&missing));
        let dbs = ctx.list_dbs().await.unwrap();
        assert!(dbs.is_empty());
    }

    #[tokio::test]
    async fn list_dbs_errors_without_db_root() {
        let ctx = AppContext::new(Config::default());
        assert!(ctx.list_dbs().await.is_err());
    }
}
