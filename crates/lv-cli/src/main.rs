use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use futures::StreamExt;
use tokio::sync::mpsc;

use lv_core::traits::CodeGraph;
use lv_core::types::{
    CompletionRequest, Message, Role, SearchFilter, SearchResult,
};
use lv_core::Config;
use lv_rag::query::QueryEngine;
use lv_tui::{run_tui, AppCommand, AppEvent};

mod app_context;
use app_context::AppContext;

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
    match &config.models.embedding {
        Some(m) => println!("  embedding: {} ({})", m.name, m.backend),
        None => println!("  embedding: (not configured — RAG disabled)"),
    }
    if let Some(ref cloud) = config.models.cloud {
        println!("  cloud:     {} ({})", cloud.model, cloud.provider);
    } else {
        println!("  cloud:     (not configured)");
    }
    Ok(())
}

async fn run_interactive(config: Config) -> anyhow::Result<()> {
    let ctx = Arc::new(AppContext::new(config));

    let (event_tx, event_rx) = mpsc::channel::<AppEvent>(128);
    let (command_tx, mut command_rx) = mpsc::channel::<AppCommand>(32);

    let handler_ctx = ctx.clone();
    let handler_event_tx = event_tx.clone();

    tokio::spawn(async move {
        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                AppCommand::Ask { query } => {
                    handle_ask(&query, &handler_ctx, &handler_event_tx).await;
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
    let ctx = AppContext::new(config);

    let (search_results, repo_map) = match ctx.embedding().await? {
        Some(embedder) => {
            let store = ctx.store().await?;
            let query_engine = QueryEngine::new(embedder, store);
            let filter = SearchFilter::default();
            let results = query_engine
                .search(
                    question,
                    ctx.config.rag.retrieval_limit,
                    ctx.config.rag.retrieval_threshold,
                    &filter,
                )
                .await
                .unwrap_or_default();
            let cwd = std::env::current_dir().unwrap_or_default();
            let graph = ctx.code_graph().await;
            let map = graph.read().await.repo_map(&cwd);
            (results, map)
        }
        None => (Vec::<SearchResult>::new(), String::new()),
    };

    let mut context = String::new();
    if !search_results.is_empty() {
        context.push_str("## Relevant code:\n");
        for r in &search_results {
            context.push_str(&format!(
                "# {} (score: {:.3})\n{}\n\n",
                r.file_path, r.score, r.text
            ));
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

    let backend = ctx.inference().await?;
    let mut stream = backend.complete(req).await.context("Completion failed")?;

    use std::io::Write;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(c) => {
                write!(out, "{}", c.delta)?;
                out.flush()?;
                if c.finished {
                    break;
                }
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
    ctx: &Arc<AppContext>,
    event_tx: &mpsc::Sender<AppEvent>,
) {
    let filter = SearchFilter::default();

    let (results, repo_map) = match ctx.embedding().await {
        Ok(Some(embedder)) => match ctx.store().await {
            Ok(store) => {
                let qe = QueryEngine::new(embedder, store);
                let r = qe
                    .search(
                        query,
                        ctx.config.rag.retrieval_limit,
                        ctx.config.rag.retrieval_threshold,
                        &filter,
                    )
                    .await
                    .unwrap_or_default();
                let cwd = std::env::current_dir().unwrap_or_default();
                let graph = ctx.code_graph().await;
                let map = graph.read().await.repo_map(&cwd);
                (r, map)
            }
            Err(e) => {
                let _ = event_tx.send(AppEvent::Error(format!("store error: {e}"))).await;
                (Vec::new(), String::new())
            }
        },
        Ok(None) => (Vec::new(), String::new()),
        Err(e) => {
            let _ = event_tx
                .send(AppEvent::Error(format!("embedding error: {e}")))
                .await;
            (Vec::new(), String::new())
        }
    };

    let _ = event_tx.send(AppEvent::SearchResults(results.clone())).await;
    let _ = event_tx.send(AppEvent::RepoMap(repo_map.clone())).await;

    let mut context = String::new();
    if !results.is_empty() {
        context.push_str("## Relevant code:\n");
        for r in &results {
            context.push_str(&format!(
                "# {} (score: {:.3})\n{}\n\n",
                r.file_path, r.score, r.text
            ));
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

    let backend = match ctx.inference().await {
        Ok(b) => b,
        Err(e) => {
            let _ = event_tx.send(AppEvent::Error(e.to_string())).await;
            return;
        }
    };

    match backend.complete(req).await {
        Ok(mut stream) => {
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
