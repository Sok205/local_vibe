use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use futures::StreamExt;
use tokio::sync::{mpsc, RwLock};

use lv_core::traits::{CodeGraph, InferenceBackend};
use lv_core::types::{
    CompletionRequest, Message, ModelTier, Role, SearchFilter,
};
use lv_core::Config;
use lv_inference::mlx_lm::MlxLmBackend;
use lv_metal::MetalBackend;
use lv_rag::code_graph::TreeSitterGraph;
use lv_rag::query::QueryEngine;
use lv_rag::store::LanceStore;
use lv_router::EscalatingRouter;
use lv_tui::{run_tui, AppCommand, AppEvent};

#[derive(Parser)]
#[command(name = "local-vibe", about = "Local AI coding assistant", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// One-shot ask with streaming output
    Ask {
        /// The question to ask
        question: String,
    },
    /// Index a directory for RAG
    Index {
        /// Path to index (default: current directory)
        path: Option<String>,
    },
    /// Start MCP server on stdio
    Serve,
    /// List configured models
    Models,
    /// Show index stats
    Stats,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let config = Config::discover();

    match cli.command {
        None => run_interactive(config).await,
        Some(Command::Ask { question }) => run_ask(config, &question).await,
        Some(Command::Index { path }) => {
            let p = path.unwrap_or_else(|| ".".to_string());
            eprintln!("[stub] Indexing path: {p} — will be wired in integration task");
            Ok(())
        }
        Some(Command::Serve) => {
            eprintln!("[stub] MCP server on stdio — will be wired in integration task");
            Ok(())
        }
        Some(Command::Models) => run_models(&config),
        Some(Command::Stats) => {
            eprintln!("[stub] Index stats — will be wired in integration task");
            Ok(())
        }
    }
}

fn run_models(config: &Config) -> anyhow::Result<()> {
    println!("Configured models:");
    println!("  fast:      {} ({})", config.models.fast.name, config.models.fast.backend);
    println!("  medium:    {} ({})", config.models.medium.name, config.models.medium.backend);
    println!("  strong:    {} ({})", config.models.strong.name, config.models.strong.backend);
    println!("  embedding: {} ({})", config.models.embedding.name, config.models.embedding.backend);
    if let Some(ref cloud) = config.models.cloud {
        println!("  cloud:     {} ({})", cloud.model, cloud.provider);
    } else {
        println!("  cloud:     (not configured)");
    }
    Ok(())
}

async fn setup(config: &Config) -> anyhow::Result<(Arc<EscalatingRouter>, Arc<QueryEngine>, Arc<RwLock<TreeSitterGraph>>)> {
    let backend: Arc<dyn InferenceBackend> = match config.models.medium.backend.as_str() {
        "metal" => {
            let model_path = config.models.medium.model_path.as_ref()
                .ok_or_else(|| anyhow::anyhow!("model_path required for metal backend"))?;
            let tokenizer_path = config.models.medium.tokenizer_path.as_ref()
                .ok_or_else(|| anyhow::anyhow!("tokenizer_path required for metal backend"))?;
            Arc::new(MetalBackend::load(model_path, tokenizer_path, ModelTier::Medium)?)
        }
        _ => {
            Arc::new(MlxLmBackend::connect(&config.models.medium.name, 8080, ModelTier::Medium))
        }
    };

    let router = Arc::new(
        EscalatingRouter::new()
            .add_tier(ModelTier::Medium, backend.clone()),
    );

    // Probe embedding dimension
    let dim: usize = match backend.embed(&["test"]).await {
        Ok(vecs) if !vecs.is_empty() => vecs[0].len(),
        _ => 768, // fallback default
    };

    let db_path = config.rag.db_dir.to_string_lossy().to_string();
    let store = Arc::new(
        LanceStore::new(&db_path, dim)
            .await
            .context("Failed to create LanceStore")?,
    );

    let query_engine = Arc::new(QueryEngine::new(backend.clone(), store));

    let code_graph = Arc::new(RwLock::new(
        TreeSitterGraph::new(&config.code_graph.languages),
    ));

    Ok((router, query_engine, code_graph))
}

async fn run_interactive(config: Config) -> anyhow::Result<()> {
    let (router, query_engine, code_graph) = setup(&config).await?;

    let (event_tx, event_rx) = mpsc::channel::<AppEvent>(128);
    let (command_tx, mut command_rx) = mpsc::channel::<AppCommand>(32);

    let handler_router = router.clone();
    let handler_query_engine = query_engine.clone();
    let handler_code_graph = code_graph.clone();
    let handler_config = config.clone();
    let handler_event_tx = event_tx.clone();

    tokio::spawn(async move {
        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                AppCommand::Ask { query } => {
                    handle_ask(
                        &query,
                        &handler_router,
                        &handler_query_engine,
                        &handler_code_graph,
                        &handler_config,
                        &handler_event_tx,
                    )
                    .await;
                }
                AppCommand::Index { path: _ } => {
                    let _ = handler_event_tx
                        .send(AppEvent::Error("Indexing not yet wired".to_string()))
                        .await;
                }
                AppCommand::Quit => break,
            }
        }
    });

    run_tui(event_rx, command_tx).await?;
    Ok(())
}

async fn run_ask(config: Config, question: &str) -> anyhow::Result<()> {
    let (router, query_engine, code_graph) = setup(&config).await?;

    // Search for context
    let filter = SearchFilter::default();
    let results = query_engine
        .search(question, config.rag.retrieval_limit, config.rag.retrieval_threshold, &filter)
        .await
        .unwrap_or_default();

    // Get repo map
    let cwd = std::env::current_dir().unwrap_or_default();
    let repo_map = code_graph.read().await.repo_map(&cwd);

    // Build context
    let mut context = String::new();
    if !results.is_empty() {
        context.push_str("## Relevant code:\n");
        for r in &results {
            context.push_str(&format!("# {} (score: {:.3})\n{}\n\n", r.file_path, r.score, r.text));
        }
    }
    if !repo_map.is_empty() {
        context.push_str("## Repository map:\n");
        context.push_str(&repo_map);
    }

    let system_msg = if context.is_empty() {
        "You are a helpful coding assistant.".to_string()
    } else {
        format!(
            "You are a helpful coding assistant. Use the following context to inform your answer:\n\n{context}"
        )
    };

    let req = CompletionRequest {
        messages: vec![
            Message { role: Role::System, content: system_msg },
            Message { role: Role::User, content: question.to_string() },
        ],
        ..Default::default()
    };

    let (_tier, mut stream) = router
        .complete_at(ModelTier::Medium, req)
        .await
        .context("Completion failed")?;

    use std::io::Write;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(c) => {
                write!(out, "{}", c.delta)?;
                out.flush()?;
            }
            Err(e) => {
                eprintln!("\nError: {e}");
                break;
            }
        }
    }
    writeln!(out)?;

    Ok(())
}

async fn handle_ask(
    query: &str,
    router: &Arc<EscalatingRouter>,
    query_engine: &Arc<QueryEngine>,
    code_graph: &Arc<RwLock<TreeSitterGraph>>,
    config: &Config,
    event_tx: &mpsc::Sender<AppEvent>,
) {
    let filter = SearchFilter::default();
    let results = query_engine
        .search(query, config.rag.retrieval_limit, config.rag.retrieval_threshold, &filter)
        .await
        .unwrap_or_default();

    let _ = event_tx.send(AppEvent::SearchResults(results.clone())).await;

    let cwd = std::env::current_dir().unwrap_or_default();
    let repo_map = code_graph.read().await.repo_map(&cwd);
    let _ = event_tx.send(AppEvent::RepoMap(repo_map.clone())).await;

    // Build context
    let mut context = String::new();
    if !results.is_empty() {
        context.push_str("## Relevant code:\n");
        for r in &results {
            context.push_str(&format!("# {} (score: {:.3})\n{}\n\n", r.file_path, r.score, r.text));
        }
    }
    if !repo_map.is_empty() {
        context.push_str("## Repository map:\n");
        context.push_str(&repo_map);
    }

    let system_msg = if context.is_empty() {
        "You are a helpful coding assistant.".to_string()
    } else {
        format!(
            "You are a helpful coding assistant. Use the following context to inform your answer:\n\n{context}"
        )
    };

    let req = CompletionRequest {
        messages: vec![
            Message { role: Role::System, content: system_msg },
            Message { role: Role::User, content: query.to_string() },
        ],
        ..Default::default()
    };

    let stream_result = router.complete_at(ModelTier::Medium, req).await;
    match stream_result {
        Ok((_tier, mut stream)) => {
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(c) => {
                        if !c.delta.is_empty() {
                            let _ = event_tx.send(AppEvent::StreamToken(c.delta)).await;
                        }
                        if c.finished {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(AppEvent::Error(e.to_string())).await;
                        break;
                    }
                }
            }
            let _ = event_tx.send(AppEvent::StreamDone).await;
        }
        Err(e) => {
            let _ = event_tx.send(AppEvent::Error(e.to_string())).await;
        }
    }
}
