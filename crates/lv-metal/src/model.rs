use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::models::quantized_llama::ModelWeights as LlamaWeights;
use candle_transformers::models::quantized_qwen2::ModelWeights as Qwen2Weights;
use lv_core::error::VibeError;
use lv_core::Result;
use std::path::Path;
use tracing::info;

/// Architecture-aware quantized model loaded from GGUF.
///
/// Detects the model architecture from GGUF metadata and uses the
/// appropriate Candle backend (quantized_llama for LLaMA/Gemma,
/// quantized_qwen2 for Qwen).
pub struct QuantizedModel {
    inner: ModelInner,
    device: Device,
}

enum ModelInner {
    Llama(LlamaWeights),
    Qwen2(Qwen2Weights),
}

impl QuantizedModel {
    /// Load a quantized model from a GGUF file.
    /// Automatically detects the architecture from GGUF metadata.
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

        // Detect architecture from GGUF metadata
        let arch = content
            .metadata
            .get("general.architecture")
            .and_then(|v| v.to_string().ok())
            .cloned()
            .unwrap_or_default();

        info!("detected architecture: {arch:?}");

        let inner = match arch.as_str() {
            "qwen2" => {
                info!("using Qwen2 quantized backend");
                let weights = Qwen2Weights::from_gguf(content, &mut file, device)
                    .map_err(|e| VibeError::Inference(format!("failed to load Qwen2 weights: {e}")))?;
                ModelInner::Qwen2(weights)
            }
            _ => {
                info!("using LLaMA quantized backend for architecture '{arch}'");
                let weights = LlamaWeights::from_gguf(content, &mut file, device)
                    .map_err(|e| VibeError::Inference(format!("failed to load LLaMA weights: {e}")))?;
                ModelInner::Llama(weights)
            }
        };

        info!("model weights loaded on {:?}", device);

        Ok(Self {
            inner,
            device: device.clone(),
        })
    }

    /// Run a forward pass. Returns logits for the last token in the sequence.
    pub fn forward(&mut self, input_ids: &Tensor, seqlen_offset: usize) -> Result<Tensor> {
        match &mut self.inner {
            ModelInner::Llama(w) => w.forward(input_ids, seqlen_offset),
            ModelInner::Qwen2(w) => w.forward(input_ids, seqlen_offset),
        }
        .map_err(|e| VibeError::Inference(format!("forward pass failed: {e}")))
    }

    /// Clear the KV cache across all layers (fast — no disk I/O).
    pub fn clear_kv_cache(&mut self) {
        match &mut self.inner {
            ModelInner::Llama(w) => w.clear_kv_cache(),
            ModelInner::Qwen2(w) => w.clear_kv_cache(),
        }
    }

    pub fn device(&self) -> &Device {
        &self.device
    }
}
