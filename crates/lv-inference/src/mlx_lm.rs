use crate::find_mlx_lm;
use async_trait::async_trait;
use futures::StreamExt;
use lv_core::error::VibeError;
use lv_core::traits::InferenceBackend;
use lv_core::types::{
    BackendHealth, CompletionChunk, CompletionRequest, CompletionStream, ModelInfo, ModelTier,
};
use lv_core::Result;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::process::Child;
use tokio::sync::RwLock;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, warn};

pub struct MlxLmBackend {
    model_name: String,
    port: u16,
    client: Client,
    process: Arc<RwLock<Option<Child>>>,
    tier: ModelTier,
}

impl MlxLmBackend {
    pub async fn new(model_name: impl Into<String>, port: u16, tier: ModelTier) -> Result<Self> {
        let model_name = model_name.into();
        let child = start_server(&model_name, port)?;
        let backend = Self {
            model_name,
            port,
            client: Client::new(),
            process: Arc::new(RwLock::new(Some(child))),
            tier,
        };
        backend.wait_for_ready().await?;
        Ok(backend)
    }

    pub fn connect(model_name: impl Into<String>, port: u16, tier: ModelTier) -> Self {
        Self {
            model_name: model_name.into(),
            port,
            client: Client::new(),
            process: Arc::new(RwLock::new(None)),
            tier,
        }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    async fn wait_for_ready(&self) -> Result<()> {
        let url = format!("{}/v1/models", self.base_url());
        for attempt in 0..60 {
            match self.client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    info!("mlx-lm server ready after {}s", attempt + 1);
                    return Ok(());
                }
                _ => {
                    debug!("waiting for mlx-lm server... attempt {}", attempt + 1);
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        Err(VibeError::BackendUnavailable(
            "mlx-lm server did not become ready within 60 seconds".into(),
        ))
    }
}

fn start_server(model_name: &str, port: u16) -> Result<Child> {
    let mlx_path = find_mlx_lm()?;
    let mut cmd = if mlx_path.as_os_str() == "python3" {
        let mut c = tokio::process::Command::new("python3");
        c.args([
            "-m",
            "mlx_lm.server",
            "--model",
            model_name,
            "--port",
            &port.to_string(),
        ]);
        c
    } else {
        let mut c = tokio::process::Command::new(mlx_path);
        c.args(["--model", model_name, "--port", &port.to_string()]);
        c
    };

    cmd.kill_on_drop(true)
        .spawn()
        .map_err(|e| VibeError::BackendUnavailable(format!("failed to spawn mlx-lm server: {e}")))
}

impl Drop for MlxLmBackend {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.process.try_write()
            && let Some(ref mut child) = *guard
        {
            let _ = child.start_kill();
        }
    }
}

// --- SSE / streaming response types ---

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamEvent {
    choices: Vec<StreamChoice>,
}

// --- Embeddings response ---

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[async_trait]
impl InferenceBackend for MlxLmBackend {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionStream> {
        let messages: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|m| {
                json!({
                    "role": serde_json::to_value(&m.role).unwrap_or(json!("user")),
                    "content": m.content,
                })
            })
            .collect();

        let body = json!({
            "model": self.model_name,
            "messages": messages,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
            "stream": true,
        });

        let url = format!("{}/v1/chat/completions", self.base_url());
        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| VibeError::Inference(format!("HTTP request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(VibeError::Inference(format!(
                "server returned {status}: {text}"
            )));
        }

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<CompletionChunk>>(64);
        let mut byte_stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut buffer = String::new();

            while let Some(chunk) = byte_stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx
                            .send(Err(VibeError::Inference(format!("stream error: {e}"))))
                            .await;
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                loop {
                    let Some(newline_pos) = buffer.find('\n') else {
                        break;
                    };
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    let data = match line.strip_prefix("data: ") {
                        Some(d) => d.to_string(),
                        None => continue,
                    };

                    if data == "[DONE]" {
                        return;
                    }

                    match serde_json::from_str::<StreamEvent>(&data) {
                        Ok(event) => {
                            if let Some(choice) = event.choices.into_iter().next() {
                                let finished = choice.finish_reason.is_some();
                                let delta = choice.delta.content.unwrap_or_default();
                                if tx.send(Ok(CompletionChunk { delta, finished })).await.is_err() {
                                    return;
                                }
                                if finished {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            warn!("failed to parse SSE event: {e} — data: {data}");
                        }
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let body = json!({
            "model": self.model_name,
            "input": texts,
        });

        let url = format!("{}/v1/embeddings", self.base_url());
        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| VibeError::Embedding(format!("HTTP request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(VibeError::Embedding(format!(
                "server returned {status}: {text}"
            )));
        }

        let resp: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| VibeError::Embedding(format!("failed to parse response: {e}")))?;

        Ok(resp.data.into_iter().map(|d| d.embedding).collect())
    }

    fn model_info(&self) -> ModelInfo {
        ModelInfo {
            name: self.model_name.clone(),
            tier: self.tier,
        }
    }

    async fn health(&self) -> BackendHealth {
        let url = format!("{}/v1/models", self.base_url());
        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => BackendHealth {
                available: true,
                model_loaded: Some(self.model_name.clone()),
            },
            _ => BackendHealth {
                available: false,
                model_loaded: None,
            },
        }
    }
}
