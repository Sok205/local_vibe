use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use lv_core::error::VibeError;
use lv_core::traits::EmbeddingBackend;
use lv_core::Result;
use std::sync::{Arc, Mutex, OnceLock};

pub struct FastEmbedBackend {
    model: Arc<Mutex<TextEmbedding>>,
    model_name: String,
    dim: OnceLock<usize>,
}

impl FastEmbedBackend {
    pub fn new(model_name: &str) -> Result<Self> {
        let embedding_model = match model_name {
            "nomic-embed-text" | "nomic-embed-text-v1.5" => EmbeddingModel::NomicEmbedTextV15,
            "bge-small-en" | "bge-small" => EmbeddingModel::BGESmallENV15,
            "bge-base-en" => EmbeddingModel::BGEBaseENV15,
            other => {
                return Err(VibeError::Embedding(format!(
                    "unsupported fastembed model '{other}'; try \
                     'nomic-embed-text' or 'bge-small-en'"
                )));
            }
        };
        let model = TextEmbedding::try_new(
            InitOptions::new(embedding_model).with_show_download_progress(true),
        )
        .map_err(|e| VibeError::Embedding(format!("fastembed init: {e}")))?;
        Ok(Self {
            model: Arc::new(Mutex::new(model)),
            model_name: model_name.to_string(),
            dim: OnceLock::new(),
        })
    }
}

#[async_trait]
impl EmbeddingBackend for FastEmbedBackend {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let texts: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        let model = Arc::clone(&self.model);
        let vectors = tokio::task::spawn_blocking(move || {
            let mut guard = model
                .lock()
                .map_err(|e| VibeError::Embedding(format!("lock poisoned: {e}")))?;
            guard
                .embed(texts, None)
                .map_err(|e| VibeError::Embedding(format!("fastembed embed: {e}")))
        })
        .await
        .map_err(|e| VibeError::Embedding(format!("spawn_blocking: {e}")))??;
        if let Some(first) = vectors.first() {
            let _ = self.dim.set(first.len());
        }
        Ok(vectors)
    }

    fn dim(&self) -> usize {
        *self.dim.get().unwrap_or(&0)
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}
