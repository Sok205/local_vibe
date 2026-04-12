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

/// Lazy holder for every expensive component. Each field initializes on first
/// access; a subcommand that does not touch embedding pays no embedding cost.
pub struct AppContext {
    pub config: Config,
    inference: OnceCell<Arc<dyn InferenceBackend>>,
    embedding: OnceCell<Option<Arc<dyn EmbeddingBackend>>>,
    store: OnceCell<Arc<dyn VectorStore>>,
    code_graph: OnceCell<Arc<RwLock<TreeSitterGraph>>>,
}

#[allow(dead_code)]
impl AppContext {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            inference: OnceCell::new(),
            embedding: OnceCell::new(),
            store: OnceCell::new(),
            code_graph: OnceCell::new(),
        }
    }
}

#[allow(dead_code)]
impl AppContext {
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

    pub async fn store(&self) -> anyhow::Result<Arc<dyn VectorStore>> {
        self.store
            .get_or_try_init(|| async {
                let embedder = self
                    .embedding
                    .get_or_try_init(|| async { build_embedding(&self.config).await })
                    .await?
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!(
                        "cannot open vector store: no embedding model configured"
                    ))?
                    .clone();
                let _ = embedder
                    .embed(&["dimension probe"])
                    .await
                    .context("embedding probe failed")?;
                let dim = embedder.dim();
                if dim == 0 {
                    anyhow::bail!("embedder returned empty vector — cannot open store");
                }
                let db_path = self.config.rag.db_dir.to_string_lossy().to_string();
                let store: Arc<dyn VectorStore> = Arc::new(
                    LanceStore::new(&db_path, dim)
                        .await
                        .context("Failed to create LanceStore")?,
                );
                Ok::<_, anyhow::Error>(store)
            })
            .await
            .cloned()
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
