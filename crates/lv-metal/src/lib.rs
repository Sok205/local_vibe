//! Apple Silicon Metal inference backend for LocalVibe, built on candle.
//! Loads GGUF quantized models and runs them through the Metal pipeline.

pub mod model;
pub mod sampler;
pub mod tokenizer;

use crate::model::QuantizedModel;
use crate::sampler::Sampler;
use crate::tokenizer::TokenizerWrapper;
use async_trait::async_trait;
use candle_core::{Device, Tensor};
use lv_core::Result;
use lv_core::error::VibeError;
use lv_core::traits::InferenceBackend;
use lv_core::types::*;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::info;
use uuid::Uuid;

struct MetalInner {
    model: QuantizedModel,
    tokenizer: TokenizerWrapper,
    last_session_id: Option<Uuid>,
}

/// Candle + Metal inference backend for GGUF models.
///
/// **Concurrency:** Generation is serialized via an internal `Mutex`. A second
/// `complete()` call waits until the first finishes. This is intentional for
/// CLI and single-tool MCP usage. For concurrent tool execution (e.g. `/agent`
/// mode spawning multiple calls), replace the mutex with a worker task that
/// owns `MetalInner` and services requests over an mpsc channel — the
/// `session_id` field on `CompletionRequest` already carries enough routing
/// information for per-session state to live in the worker.
pub struct MetalBackend {
    inner: Arc<Mutex<MetalInner>>,
    model_name: String,
    tier: ModelTier,
}

impl MetalBackend {
    pub fn load(model_path: &Path, tokenizer_path: &Path, tier: ModelTier) -> Result<Self> {
        // TODO: Metal device has incomplete kernel support in Candle 0.8
        // (missing rms-norm). Use CPU until we upgrade Candle or add custom kernels.
        let device = if std::env::var("LV_FORCE_CPU").is_ok() {
            info!("LV_FORCE_CPU set, using CPU device");
            Device::Cpu
        } else {
            match Device::new_metal(0) {
                Ok(d) => {
                    info!("Metal device created");
                    d
                }
                Err(e) => {
                    info!("Metal device failed ({e}), falling back to CPU");
                    Device::Cpu
                }
            }
        };

        info!("using device: {:?}", device);

        let model = QuantizedModel::load(model_path, &device)?;
        let tokenizer = TokenizerWrapper::from_file(tokenizer_path)?;

        let model_name = model_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("gemma4")
            .to_string();

        info!("MetalBackend loaded: {}", model_name);

        Ok(Self {
            inner: Arc::new(Mutex::new(MetalInner {
                model,
                tokenizer,
                last_session_id: None,
            })),
            model_name,
            tier,
        })
    }
}

#[async_trait]
impl InferenceBackend for MetalBackend {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionStream> {
        let inner = Arc::clone(&self.inner);

        // Tokenize the prompt while holding the lock briefly
        let (input_ids, device) = {
            let guard = inner
                .lock()
                .map_err(|e| VibeError::Inference(format!("lock poisoned: {e}")))?;
            let prompt = guard.tokenizer.apply_chat_template(&req.messages);
            let tokens = guard.tokenizer.encode(&prompt)?;
            let device = guard.model.device().clone();
            (tokens, device)
        };

        let max_tokens = req.max_tokens;
        let temperature = req.temperature as f64;
        let session = req.session_id;

        let (tx, rx) = mpsc::channel::<Result<CompletionChunk>>(32);

        tokio::task::spawn_blocking(move || {
            let mut sampler = Sampler::new(42, temperature, Some(0.95), Some(40));

            let lock_start = std::time::Instant::now();
            let mut guard = match inner.lock() {
                Ok(g) => {
                    let waited = lock_start.elapsed();
                    if waited > std::time::Duration::from_millis(50) {
                        tracing::warn!(
                            "MetalBackend serialized generation for {waited:?} — consider worker-task \
                             refactor if this becomes common"
                        );
                    }
                    g
                }
                Err(e) => {
                    let _ =
                        tx.blocking_send(Err(VibeError::Inference(format!("lock poisoned: {e}"))));
                    return;
                }
            };

            let should_clear = match (guard.last_session_id, session) {
                (Some(prev), Some(curr)) if prev == curr => false, // same session: keep KV warm
                _ => true,                                         // new/unsessioned: clear
            };
            if should_clear {
                guard.model.clear_kv_cache();
            }
            guard.last_session_id = session;

            // Prefill: run forward on full prompt
            let prompt_len = input_ids.len();
            let input_tensor =
                match Tensor::new(input_ids.as_slice(), &device).and_then(|t| t.unsqueeze(0)) {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = tx.blocking_send(Err(VibeError::Inference(format!(
                            "tensor creation failed: {e}"
                        ))));
                        return;
                    }
                };

            let logits = match guard.model.forward(&input_tensor, 0) {
                Ok(l) => l,
                Err(e) => {
                    let _ = tx.blocking_send(Err(e));
                    return;
                }
            };

            // Squeeze batch dim: [1, vocab] -> [vocab]
            let mut logits = match logits.squeeze(0) {
                Ok(l) => l,
                Err(e) => {
                    let _ =
                        tx.blocking_send(Err(VibeError::Inference(format!("squeeze failed: {e}"))));
                    return;
                }
            };

            // Decode loop
            for i in 0..max_tokens {
                let token_id = match sampler.sample(&logits) {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = tx.blocking_send(Err(e));
                        return;
                    }
                };

                if guard.tokenizer.is_eos(token_id) {
                    let _ = tx.blocking_send(Ok(CompletionChunk {
                        delta: String::new(),
                        finished: true,
                    }));
                    return;
                }

                let text = match guard.tokenizer.decode(&[token_id]) {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = tx.blocking_send(Err(e));
                        return;
                    }
                };

                if tx
                    .blocking_send(Ok(CompletionChunk {
                        delta: text,
                        finished: false,
                    }))
                    .is_err()
                {
                    return; // receiver dropped
                }

                // Next forward pass with single token
                let next_input =
                    match Tensor::new(&[token_id], &device).and_then(|t| t.unsqueeze(0)) {
                        Ok(t) => t,
                        Err(e) => {
                            let _ = tx.blocking_send(Err(VibeError::Inference(format!(
                                "tensor creation failed: {e}"
                            ))));
                            return;
                        }
                    };

                let raw_logits = match guard.model.forward(&next_input, prompt_len + i as usize) {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = tx.blocking_send(Err(e));
                        return;
                    }
                };
                logits = match raw_logits.squeeze(0) {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = tx.blocking_send(Err(VibeError::Inference(format!(
                            "squeeze failed: {e}"
                        ))));
                        return;
                    }
                };
            }

            // Hit max tokens
            let _ = tx.blocking_send(Ok(CompletionChunk {
                delta: String::new(),
                finished: true,
            }));
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn model_info(&self) -> ModelInfo {
        ModelInfo {
            name: self.model_name.clone(),
            tier: self.tier,
        }
    }

    async fn health(&self) -> BackendHealth {
        BackendHealth {
            available: true,
            model_loaded: Some(self.model_name.clone()),
        }
    }
}
