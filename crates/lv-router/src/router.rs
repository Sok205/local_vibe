use lv_core::traits::InferenceBackend;
use lv_core::types::*;
use lv_core::Result;
use std::sync::Arc;
use tracing::info;

pub struct EscalatingRouter {
    tiers: Vec<(ModelTier, Arc<dyn InferenceBackend>)>,
}

impl EscalatingRouter {
    pub fn new() -> Self {
        Self { tiers: Vec::new() }
    }

    pub fn add_tier(mut self, tier: ModelTier, backend: Arc<dyn InferenceBackend>) -> Self {
        self.tiers.push((tier, backend));
        self.tiers.sort_by_key(|(t, _)| *t);
        self
    }

    /// Get a backend at a specific tier or higher.
    pub fn backend_at(&self, min_tier: ModelTier) -> Option<&Arc<dyn InferenceBackend>> {
        self.tiers.iter()
            .find(|(t, _)| *t >= min_tier)
            .map(|(_, b)| b)
    }

    /// Get the best available backend (highest tier that is healthy).
    pub async fn best_available(&self) -> Option<(ModelTier, &Arc<dyn InferenceBackend>)> {
        for (tier, backend) in self.tiers.iter().rev() {
            if backend.health().await.available {
                return Some((*tier, backend));
            }
        }
        None
    }

    /// Complete a request starting at the given tier.
    pub async fn complete_at(&self, tier: ModelTier, req: CompletionRequest) -> Result<(ModelTier, CompletionStream)> {
        let backend = self.backend_at(tier)
            .ok_or_else(|| lv_core::VibeError::BackendUnavailable(
                format!("no backend available at tier {tier:?} or above")
            ))?;

        let used_tier = self.tiers.iter()
            .find(|(_, b)| Arc::ptr_eq(b, backend))
            .map(|(t, _)| *t)
            .unwrap_or(tier);

        info!("routing to {:?} ({})", used_tier, backend.model_info().name);
        let stream = backend.complete(req).await?;
        Ok((used_tier, stream))
    }

    /// Get embedding from the first available backend.
    pub async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let (_, backend) = self.tiers.first()
            .ok_or_else(|| lv_core::VibeError::BackendUnavailable("no backends registered".into()))?;
        backend.embed(texts).await
    }

    pub fn list_models(&self) -> Vec<(ModelTier, ModelInfo)> {
        self.tiers.iter().map(|(t, b)| (*t, b.model_info())).collect()
    }
}
