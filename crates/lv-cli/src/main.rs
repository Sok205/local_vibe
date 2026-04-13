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
#[command(name = "lv", about = "Local AI coding assistant", version)]
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
    let cli = Cli::parse();
    let is_tui = cli.command.is_none();
    let is_serve = matches!(cli.command, Some(Command::Serve));

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    if is_tui || is_serve {
        // TUI owns the terminal; serve owns stdout for JSON-RPC frames. Either way,
        // route logs to a file so they don't corrupt the protocol or the UI.
        let log_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("local-vibe");
        std::fs::create_dir_all(&log_dir).ok();
        let log_name = if is_serve { "lv-mcp.log" } else { "lv.log" };
        let log_path = log_dir.join(log_name);
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("failed to open log file {}", log_path.display()))?;
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(log_file))
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .init();
    }

    let config = Config::discover();

    match cli.command {
        None => run_interactive(config).await,
        Some(Command::Ask { question }) => run_ask(config, &question).await,
        Some(Command::Index { path }) => {
            let p = path.unwrap_or_else(|| ".".to_string());
            run_index(config, &p).await
        }
        Some(Command::Serve) => run_serve(config).await,
        Some(Command::Models) => run_models(&config),
        Some(Command::Stats) => run_stats(config).await,
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
    let session_id = uuid::Uuid::new_v4();

    let (event_tx, event_rx) = mpsc::channel::<AppEvent>(128);
    let (command_tx, mut command_rx) = mpsc::channel::<AppCommand>(32);

    let handler_ctx = ctx.clone();
    let handler_event_tx = event_tx.clone();

    tokio::spawn(async move {
        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                AppCommand::Ask { query } => {
                    handle_ask(&query, session_id, &handler_ctx, &handler_event_tx).await;
                }
                AppCommand::Index { path, db } => {
                    index_with_progress(&handler_ctx, &path, db, &handler_event_tx).await;
                }
                AppCommand::ListDbs => {
                    let names = handler_ctx.list_dbs().await.unwrap_or_default();
                    let _ = handler_event_tx.send(AppEvent::DbListing(names)).await;
                }
                AppCommand::SwitchDb(name) => {
                    match handler_ctx.set_current_db(&name).await {
                        Ok(()) => {
                            let _ = handler_event_tx.send(AppEvent::DbSwitched(name)).await;
                        }
                        Err(e) => {
                            let _ = handler_event_tx.send(AppEvent::Error(e.to_string())).await;
                        }
                    }
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
        session_id: Some(uuid::Uuid::new_v4()),
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
    session_id: uuid::Uuid,
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
        session_id: Some(session_id),
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

async fn index_with_progress(
    ctx: &Arc<AppContext>,
    path: &str,
    db_name: Option<String>,
    event_tx: &mpsc::Sender<AppEvent>,
) {
    use lv_rag::chunker::OverlappingChunker;
    use lv_rag::indexer::IndexManager;
    use lv_rag::parsers::{epub::EpubParser, html::HtmlParser, pdf::PdfParser, text::TextParser};

    let embedder = match ctx.embedding().await {
        Ok(Some(e)) => e,
        Ok(None) => {
            let _ = event_tx
                .send(AppEvent::Error("no embedding model configured".into()))
                .await;
            return;
        }
        Err(e) => {
            let _ = event_tx
                .send(AppEvent::Error(format!("embedding: {e}")))
                .await;
            return;
        }
    };
    let store = match db_name {
        Some(ref name) => match ctx.store_named(name).await {
            Ok(s) => s,
            Err(e) => {
                let _ = event_tx.send(AppEvent::Error(format!("store: {e}"))).await;
                return;
            }
        },
        None => match ctx.store().await {
            Ok(s) => s,
            Err(e) => {
                let _ = event_tx.send(AppEvent::Error(format!("store: {e}"))).await;
                return;
            }
        },
    };

    let parsers: Vec<Box<dyn lv_core::traits::Parser>> = vec![
        Box::new(TextParser),
        Box::new(PdfParser),
        Box::new(HtmlParser),
        Box::new(EpubParser),
    ];
    let chunker = Box::new(OverlappingChunker::new(200, 40));
    let manager = IndexManager::new(parsers, chunker, embedder, store, 4);

    let dir = std::path::Path::new(path);
    let (mut rx, handle, _cancel) = match manager.index(dir).await {
        Ok(x) => x,
        Err(e) => {
            let _ = event_tx
                .send(AppEvent::Error(format!("index start: {e}")))
                .await;
            return;
        }
    };
    while let Some(progress) = rx.recv().await {
        match progress {
            lv_core::types::IndexProgress::Indexing { done, total, current } => {
                let _ = event_tx
                    .send(AppEvent::IndexProgress { done, total, current })
                    .await;
            }
            lv_core::types::IndexProgress::Complete { indexed, skipped, failed } => {
                let _ = event_tx
                    .send(AppEvent::IndexDone { indexed, skipped, failed })
                    .await;
            }
            lv_core::types::IndexProgress::Error(e) => {
                let _ = event_tx.send(AppEvent::Error(e)).await;
            }
            _ => {}
        }
    }
    let _ = handle.await;
}

async fn run_index(config: Config, path: &str) -> anyhow::Result<()> {
    use lv_rag::chunker::OverlappingChunker;
    use lv_rag::indexer::IndexManager;
    use lv_rag::parsers::{epub::EpubParser, html::HtmlParser, pdf::PdfParser, text::TextParser};

    let ctx = AppContext::new(config);
    let Some(embedder) = ctx.embedding().await? else {
        anyhow::bail!(
            "cannot index: no embedding model configured. \
             Set [models.embedding] in local-vibe.toml to enable RAG."
        );
    };
    let store = ctx.store().await?;

    let parsers: Vec<Box<dyn lv_core::traits::Parser>> = vec![
        Box::new(TextParser),
        Box::new(PdfParser),
        Box::new(HtmlParser),
        Box::new(EpubParser),
    ];
    let chunker = Box::new(OverlappingChunker::new(200, 40));
    let manager = IndexManager::new(parsers, chunker, embedder, store, 4);

    let dir = std::path::Path::new(path);
    let (mut rx, handle, _cancel) = manager.index(dir).await?;
    while let Some(progress) = rx.recv().await {
        match progress {
            lv_core::types::IndexProgress::Indexing { done, total, current } => {
                eprintln!("[{done}/{total}] {current}");
            }
            lv_core::types::IndexProgress::Complete { indexed, skipped, failed } => {
                eprintln!("Indexed: {indexed}, skipped: {skipped}, failed: {failed}");
            }
            _ => {}
        }
    }
    handle.await??;
    Ok(())
}

async fn run_serve(config: Config) -> anyhow::Result<()> {
    let ctx = AppContext::new(config);
    let embedder = ctx
        .embedding()
        .await
        .context("embedding backend init")?
        .context(
            "MCP serve requires an embedding model. \
             Set [models.embedding] in local-vibe.toml.",
        )?;
    let store = ctx.store().await.context("vector store init")?;

    tracing::info!("MCP server starting on stdio");
    let server = lv_mcp::VibeMcpServer::new(embedder, store);
    lv_mcp::run_stdio(server)
        .await
        .map_err(|e| anyhow::anyhow!("MCP server: {e}"))?;
    tracing::info!("MCP server stopped");
    Ok(())
}

async fn run_stats(config: Config) -> anyhow::Result<()> {
    let ctx = AppContext::new(config);
    if ctx.embedding().await?.is_none() {
        println!("RAG disabled (no [models.embedding] configured).");
        return Ok(());
    }
    let store = ctx.store().await?;
    let stats = store.stats().await?;
    println!(
        "Indexed store: {} chunks from {} unique files",
        stats.total_chunks, stats.unique_files
    );
    Ok(())
}
