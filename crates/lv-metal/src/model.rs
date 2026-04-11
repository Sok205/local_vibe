use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::models::quantized_llama::ModelWeights;
use lv_core::error::VibeError;
use lv_core::Result;
use std::path::Path;
use tracing::info;

/// Quantized Gemma 4 model backed by Candle's quantized llama implementation.
///
/// Candle 0.8 does not ship a dedicated quantized Gemma model. The quantized llama
/// implementation reads architecture-specific config (head counts, embedding size,
/// RoPE parameters, etc.) from GGUF metadata, so it works for Gemma-family GGUF
/// files produced by llama.cpp.
pub struct QuantizedGemma4 {
    weights: ModelWeights,
    device: Device,
    model_path: std::path::PathBuf,
}

impl QuantizedGemma4 {
    /// Load a quantized model from a GGUF file.
    pub fn load(model_path: &Path, device: &Device) -> Result<Self> {
        info!("loading GGUF model from {}", model_path.display());

        let mut file = std::fs::File::open(model_path)
            .map_err(|e| VibeError::Inference(format!("failed to open model: {e}")))?;

        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| VibeError::Inference(format!("failed to read GGUF: {e}")))?;

        info!(
            "GGUF loaded: {} tensors, {} metadata entries",
            content.tensor_infos.len(),
            content.metadata.len()
        );

        let weights = ModelWeights::from_gguf(content, &mut file, device)
            .map_err(|e| VibeError::Inference(format!("failed to load weights: {e}")))?;

        info!("model weights loaded on {:?}", device);

        Ok(Self {
            weights,
            device: device.clone(),
            model_path: model_path.to_path_buf(),
        })
    }

    /// Run a forward pass. Returns logits for the last token in the sequence.
    ///
    /// `input_ids` shape: `(batch, seq_len)`
    /// `seqlen_offset`: position offset for KV cache (0 for prompt, increments for generation).
    pub fn forward(&mut self, input_ids: &Tensor, seqlen_offset: usize) -> Result<Tensor> {
        self.weights
            .forward(input_ids, seqlen_offset)
            .map_err(|e| VibeError::Inference(format!("forward pass failed: {e}")))
    }

    /// Clear the KV cache by reloading the model weights.
    ///
    /// Candle 0.8's `quantized_llama::ModelWeights` does not expose a public
    /// method to clear the per-layer KV cache. As a workaround we reload from
    /// disk. This is acceptable because cache clearing only happens between
    /// conversations, not on the hot path.
    pub fn clear_kv_cache(&mut self) {
        info!("clearing KV cache by reloading model");
        match Self::load(&self.model_path, &self.device) {
            Ok(fresh) => self.weights = fresh.weights,
            Err(e) => {
                tracing::error!("failed to reload model for cache clear: {e}");
            }
        }
    }

    pub fn device(&self) -> &Device {
        &self.device
    }
}
