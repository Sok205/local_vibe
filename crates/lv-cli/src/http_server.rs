use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use futures::{Stream, StreamExt};
use lv_core::types::{CompletionRequest, Message, ModelTier, Role};
use serde_json::json;
use tower_http::cors::CorsLayer;

use crate::app_context::AppContext;
use crate::openai::*;

#[derive(Clone)]
struct HttpState {
    ctx: Arc<AppContext>,
}

pub async fn serve_http(
    ctx: Arc<AppContext>,
    addr: SocketAddr,
    preload: Option<ModelTier>,
) -> anyhow::Result<()> {
    if let Some(tier) = preload {
        tracing::info!("pre-loading tier {:?} before HTTP server start", tier);
        ctx.load_model(tier)
            .await
            .with_context(|| format!("failed to pre-load tier {tier:?}"))?;
        ctx.set_active_tier(tier).await.ok();
    }

    let state = HttpState { ctx };
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .layer(CorsLayer::permissive())
        .with_state(state);

    tracing::info!("HTTP server listening on http://{}", addr);
    eprintln!("lv http listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    axum::serve(listener, app)
        .await
        .context("axum server crashed")?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn list_models(State(state): State<HttpState>) -> Json<ModelsResponse> {
    let cfg = &state.ctx.config;
    let created = unix_now();
    let mut data = vec![
        ModelEntry {
            id: "fast".to_string(),
            object: "model",
            created,
            owned_by: "lv",
        },
        ModelEntry {
            id: "medium".to_string(),
            object: "model",
            created,
            owned_by: "lv",
        },
        ModelEntry {
            id: "strong".to_string(),
            object: "model",
            created,
            owned_by: "lv",
        },
    ];
    for (alias, slot_name) in [
        ("fast", &cfg.models.fast.name),
        ("medium", &cfg.models.medium.name),
        ("strong", &cfg.models.strong.name),
    ] {
        if slot_name != alias {
            data.push(ModelEntry {
                id: slot_name.clone(),
                object: "model",
                created,
                owned_by: "lv",
            });
        }
    }
    Json(ModelsResponse {
        object: "list",
        data,
    })
}

async fn chat_completions(
    State(state): State<HttpState>,
    Json(req): Json<ChatCompletionRequest>,
) -> Response {
    let tier = resolve_tier(&req.model, &state.ctx.config);

    if let Err(e) = state.ctx.load_model(tier).await {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("load model: {e}"),
        );
    }
    if let Err(e) = state.ctx.set_active_tier(tier).await {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("set active tier: {e}"),
        );
    }

    let backend = match state.ctx.active_inference().await {
        Ok(b) => b,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("active inference: {e}"),
            );
        }
    };

    let model_label = req.model.clone();
    let has_tools = req.tools.as_ref().is_some_and(|t| !t.is_empty());
    let stream_mode = req.stream.unwrap_or(false) && !has_tools;

    let lv_messages = openai_messages_to_lv(&req.messages, req.tools.as_deref());
    let lv_req = CompletionRequest {
        messages: lv_messages,
        temperature: req.temperature.unwrap_or(0.7),
        max_tokens: req.max_tokens.unwrap_or(4096),
        stream: true,
        session_id: Some(uuid::Uuid::new_v4()),
    };

    let stream = match backend.complete(lv_req).await {
        Ok(s) => s,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("completion: {e}"),
            );
        }
    };

    if stream_mode {
        sse_response(stream, model_label).into_response()
    } else {
        match collect_response(stream, model_label, has_tools).await {
            Ok(resp) => Json(resp).into_response(),
            Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("stream: {e}")),
        }
    }
}

fn sse_response(
    mut stream: lv_core::types::CompletionStream,
    model: String,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let created = unix_now();
    let id_clone = id.clone();
    let model_clone = model.clone();

    let s = async_stream::stream! {
        // Initial role chunk so the client sees role: assistant.
        let opener = StreamChunk {
            id: id_clone.clone(),
            object: "chat.completion.chunk",
            created,
            model: model_clone.clone(),
            choices: vec![StreamChoice {
                index: 0,
                delta: StreamDelta { role: Some("assistant".into()), ..Default::default() },
                finish_reason: None,
            }],
        };
        if let Ok(json) = serde_json::to_string(&opener) {
            yield Ok::<_, Infallible>(Event::default().data(json));
        }

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(c) => {
                    if !c.delta.is_empty() {
                        let frame = StreamChunk {
                            id: id_clone.clone(),
                            object: "chat.completion.chunk",
                            created,
                            model: model_clone.clone(),
                            choices: vec![StreamChoice {
                                index: 0,
                                delta: StreamDelta {
                                    content: Some(c.delta),
                                    ..Default::default()
                                },
                                finish_reason: None,
                            }],
                        };
                        if let Ok(json) = serde_json::to_string(&frame) {
                            yield Ok(Event::default().data(json));
                        }
                    }
                    if c.finished {
                        break;
                    }
                }
                Err(e) => {
                    let err = json!({ "error": { "message": e.to_string() } });
                    yield Ok(Event::default().data(err.to_string()));
                    break;
                }
            }
        }

        let closer = StreamChunk {
            id: id_clone,
            object: "chat.completion.chunk",
            created,
            model: model_clone,
            choices: vec![StreamChoice {
                index: 0,
                delta: StreamDelta::default(),
                finish_reason: Some("stop".into()),
            }],
        };
        if let Ok(json) = serde_json::to_string(&closer) {
            yield Ok(Event::default().data(json));
        }
        yield Ok(Event::default().data("[DONE]"));
    };

    Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

async fn collect_response(
    mut stream: lv_core::types::CompletionStream,
    model: String,
    has_tools: bool,
) -> anyhow::Result<ChatCompletionResponse> {
    let mut full = String::new();
    while let Some(chunk) = stream.next().await {
        let c = chunk.map_err(|e| anyhow::anyhow!(e))?;
        full.push_str(&c.delta);
        if c.finished {
            break;
        }
    }

    let (content, tool_calls) = if has_tools {
        parse_tool_calls(&full)
    } else {
        (full, Vec::new())
    };

    let finish_reason = if !tool_calls.is_empty() {
        "tool_calls".to_string()
    } else {
        "stop".to_string()
    };

    Ok(ChatCompletionResponse {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        object: "chat.completion",
        created: unix_now(),
        model,
        choices: vec![Choice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: if content.is_empty() {
                    None
                } else {
                    Some(content)
                },
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                tool_call_id: None,
                name: None,
            },
            finish_reason: Some(finish_reason),
        }],
        usage: None,
    })
}

fn resolve_tier(model: &str, cfg: &lv_core::Config) -> ModelTier {
    match model.to_lowercase().as_str() {
        "fast" => return ModelTier::Fast,
        "medium" => return ModelTier::Medium,
        "strong" => return ModelTier::Strong,
        _ => {}
    }
    if model == cfg.models.fast.name {
        ModelTier::Fast
    } else if model == cfg.models.medium.name {
        ModelTier::Medium
    } else if model == cfg.models.strong.name {
        ModelTier::Strong
    } else {
        ModelTier::Medium
    }
}

fn openai_messages_to_lv(messages: &[ChatMessage], tools: Option<&[Tool]>) -> Vec<Message> {
    let mut out: Vec<Message> = Vec::with_capacity(messages.len() + 1);

    let mut first_system_handled = false;
    if let Some(tool_block) = tools
        .filter(|t| !t.is_empty())
        .map(format_tools_system_prompt)
    {
        if let Some(first) = messages.first()
            && first.role == "system"
        {
            let original = first.content.clone().unwrap_or_default();
            out.push(Message {
                role: Role::System,
                content: format!("{original}\n\n{tool_block}"),
            });
            first_system_handled = true;
        } else {
            out.push(Message {
                role: Role::System,
                content: tool_block,
            });
        }
    }

    for (i, msg) in messages.iter().enumerate() {
        if i == 0 && first_system_handled {
            continue;
        }
        let role = match msg.role.as_str() {
            "system" => Role::System,
            "assistant" => Role::Assistant,
            "tool" => Role::User,
            _ => Role::User,
        };

        let content = if let Some(calls) = &msg.tool_calls {
            let mut s = msg.content.clone().unwrap_or_default();
            for c in calls {
                let args_val: serde_json::Value = serde_json::from_str(&c.function.arguments)
                    .unwrap_or(serde_json::Value::String(c.function.arguments.clone()));
                let payload = json!({ "name": c.function.name, "arguments": args_val });
                if !s.is_empty() {
                    s.push('\n');
                }
                s.push_str(&format!("<tool_call>\n{payload}\n</tool_call>"));
            }
            s
        } else if msg.role == "tool" {
            let body = msg.content.clone().unwrap_or_default();
            format!("<tool_response>\n{body}\n</tool_response>")
        } else {
            msg.content.clone().unwrap_or_default()
        };

        out.push(Message { role, content });
    }
    out
}

fn format_tools_system_prompt(tools: &[Tool]) -> String {
    let arr: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            json!({
                "type": t.kind,
                "function": {
                    "name": t.function.name,
                    "description": t.function.description,
                    "parameters": t.function.parameters,
                }
            })
        })
        .collect();
    let pretty = serde_json::to_string_pretty(&arr).unwrap_or_else(|_| "[]".into());
    format!(
        "# Tools\n\nYou have access to the following functions. \
         To call one, emit a JSON object inside <tool_call></tool_call> tags. \
         You may call multiple tools in one turn by emitting multiple tags.\n\n\
         <tools>\n{pretty}\n</tools>\n\n\
         Format:\n<tool_call>\n{{\"name\": \"<fn_name>\", \"arguments\": {{...}}}}\n</tool_call>"
    )
}

fn parse_tool_calls(text: &str) -> (String, Vec<ToolCall>) {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";

    let mut cleaned = String::new();
    let mut calls = Vec::new();
    let mut counter: usize = 0;
    let mut rest = text;

    while let Some(start) = rest.find(OPEN) {
        cleaned.push_str(&rest[..start]);
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find(CLOSE) else {
            cleaned.push_str(&rest[start..]);
            return (cleaned.trim().to_string(), calls);
        };
        let body = after[..end].trim();
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(body) {
            let name = val
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let arguments = val
                .get("arguments")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "{}".to_string());
            calls.push(ToolCall {
                id: format!("call_{counter}"),
                kind: "function".to_string(),
                function: ToolCallFunction { name, arguments },
            });
            counter += 1;
        } else {
            cleaned.push_str(OPEN);
            cleaned.push_str(body);
            cleaned.push_str(CLOSE);
        }
        rest = &after[end + CLOSE.len()..];
    }
    cleaned.push_str(rest);
    (cleaned.trim().to_string(), calls)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn error_response(status: StatusCode, message: String) -> Response {
    let body = json!({ "error": { "message": message } });
    (status, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_tool_call() {
        let text = "Sure thing.\n<tool_call>\n{\"name\":\"get_weather\",\"arguments\":{\"city\":\"Paris\"}}\n</tool_call>";
        let (content, calls) = parse_tool_calls(text);
        assert_eq!(content, "Sure thing.");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "get_weather");
        assert!(calls[0].function.arguments.contains("Paris"));
    }

    #[test]
    fn parses_multiple_tool_calls() {
        let text = "<tool_call>\n{\"name\":\"a\",\"arguments\":{}}\n</tool_call>\n<tool_call>\n{\"name\":\"b\",\"arguments\":{\"x\":1}}\n</tool_call>";
        let (_, calls) = parse_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "a");
        assert_eq!(calls[1].function.name, "b");
    }

    #[test]
    fn unterminated_tool_call_is_left_as_text() {
        let text = "hello <tool_call>\n{not closed";
        let (content, calls) = parse_tool_calls(text);
        assert!(calls.is_empty());
        assert!(content.contains("hello"));
    }

    #[test]
    fn no_tool_calls_returns_input() {
        let (content, calls) = parse_tool_calls("just text");
        assert_eq!(content, "just text");
        assert!(calls.is_empty());
    }

    #[test]
    fn resolve_tier_aliases() {
        let cfg = lv_core::Config::default();
        assert_eq!(resolve_tier("fast", &cfg), ModelTier::Fast);
        assert_eq!(resolve_tier("MEDIUM", &cfg), ModelTier::Medium);
        assert_eq!(resolve_tier("strong", &cfg), ModelTier::Strong);
        assert_eq!(resolve_tier("unknown-model", &cfg), ModelTier::Medium);
    }
}
