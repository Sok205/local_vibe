use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use futures::StreamExt;
use tokio::sync::mpsc;

use lv_core::status::{collect_declared_status, Readiness, StatusSnapshot};
use lv_core::traits::{AppHost, CodeGraph};
use lv_core::types::{
    CompletionRequest, Message, ModelTier, Role, SearchFilter, SearchResult,
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
    Serve {
        /// Pre-load a chat tier at startup (fast, medium, strong).
        /// Omit to keep all models lazy (load on first use).
        #[arg(long, value_name = "TIER")]
        tier: Option<String>,
    },
    /// List configured models
    Models,
    /// Show index stats for the current DB
    Stats,
    /// Show a full status snapshot (models + every indexed DB)
    Status {
        /// Emit JSON instead of a human-readable summary
        #[arg(long)]
        json: bool,
    },
    /// List indexed DB names
    Dbs {
        #[arg(long)]
        json: bool,
    },
    /// List files inside a given DB
    Ls {
        /// DB name (or "default" when no db_root is configured)
        db: String,
        /// Max files to display
        #[arg(long, default_value_t = 500)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let is_tui = cli.command.is_none();
    let is_serve = matches!(cli.command, Some(Command::Serve { .. }));

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
        Some(Command::Serve { tier }) => run_serve(config, tier).await,
        Some(Command::Models) => run_models(&config),
        Some(Command::Stats) => run_stats(config).await,
        Some(Command::Status { json }) => run_status(config, json).await,
        Some(Command::Dbs { json }) => run_dbs(config, json).await,
        Some(Command::Ls { db, limit, json }) => run_ls(config, &db, limit, json).await,
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
                AppCommand::Status => {
                    let current = handler_ctx.current_db().await;
                    match collect_declared_status(&*handler_ctx, Some(&current)).await {
                        Ok(mut snap) => {
                            if let Some(rt) = snap.runtime.as_mut() {
                                rt.session_id = Some(session_id.to_string());
                            }
                            let _ = handler_event_tx
                                .send(AppEvent::Status(Box::new(snap)))
                                .await;
                        }
                        Err(e) => {
                            let _ = handler_event_tx
                                .send(AppEvent::Error(format!("status: {e}")))
                                .await;
                        }
                    }
                }
                AppCommand::Help | AppCommand::OpenPicker => {
                    // Handled entirely inside the TUI; never reaches the channel.
                }
                AppCommand::Models => {
                    let rows = build_model_rows(&handler_ctx).await;
                    let _ = handler_event_tx.send(AppEvent::ModelsSnapshot(rows)).await;
                }
                AppCommand::Browse(db) => {
                    let db = if db.is_empty() {
                        handler_ctx.current_db().await
                    } else {
                        db
                    };
                    match handler_ctx.open_store_readonly(&db).await {
                        Ok(store) => {
                            let files = store.list_files(usize::MAX).await.unwrap_or_default();
                            let chunks = store
                                .stats()
                                .await
                                .map(|s| s.total_chunks)
                                .unwrap_or(0);
                            let _ = handler_event_tx
                                .send(AppEvent::BrowseData {
                                    db,
                                    files,
                                    total_chunks: chunks,
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = handler_event_tx
                                .send(AppEvent::Error(format!("browse '{db}': {e}")))
                                .await;
                        }
                    }
                }
                AppCommand::LoadAndActivate(tier) => {
                    let _ = handler_event_tx.send(AppEvent::ModelLoading(tier)).await;
                    emit_models_snapshot(&handler_ctx, &handler_event_tx).await;
                    match handler_ctx.load_model(tier).await {
                        Ok(()) => {
                            let _ = handler_event_tx.send(AppEvent::ModelLoaded(tier)).await;
                            if let Err(e) = handler_ctx.set_active_tier(tier).await {
                                let _ = handler_event_tx
                                    .send(AppEvent::Error(e.to_string()))
                                    .await;
                            } else {
                                let name = active_tier_display(&handler_ctx, tier);
                                let _ = handler_event_tx
                                    .send(AppEvent::ActiveTierChanged(tier, name))
                                    .await;
                            }
                            emit_models_snapshot(&handler_ctx, &handler_event_tx).await;
                            emit_warm_count(&handler_ctx, &handler_event_tx).await;
                        }
                        Err(e) => {
                            let _ = handler_event_tx
                                .send(AppEvent::ModelLoadFailed(tier, e.to_string()))
                                .await;
                            emit_models_snapshot(&handler_ctx, &handler_event_tx).await;
                            emit_warm_count(&handler_ctx, &handler_event_tx).await;
                        }
                    }
                }
                AppCommand::LoadModel(tier) => {
                    let _ = handler_event_tx.send(AppEvent::ModelLoading(tier)).await;
                    emit_models_snapshot(&handler_ctx, &handler_event_tx).await;
                    match handler_ctx.load_model(tier).await {
                        Ok(()) => {
                            let _ = handler_event_tx.send(AppEvent::ModelLoaded(tier)).await;
                        }
                        Err(e) => {
                            let _ = handler_event_tx
                                .send(AppEvent::ModelLoadFailed(tier, e.to_string()))
                                .await;
                        }
                    }
                    emit_models_snapshot(&handler_ctx, &handler_event_tx).await;
                    emit_warm_count(&handler_ctx, &handler_event_tx).await;
                }
                AppCommand::UnloadModel(tier) => {
                    match handler_ctx.unload_model(tier).await {
                        Ok(()) => {
                            let _ = handler_event_tx.send(AppEvent::ModelUnloaded(tier)).await;
                        }
                        Err(e) => {
                            let _ = handler_event_tx
                                .send(AppEvent::Error(e.to_string()))
                                .await;
                        }
                    }
                    emit_models_snapshot(&handler_ctx, &handler_event_tx).await;
                    emit_warm_count(&handler_ctx, &handler_event_tx).await;
                }
                AppCommand::SetActiveTier(tier) => {
                    match handler_ctx.set_active_tier(tier).await {
                        Ok(()) => {
                            let name = active_tier_display(&handler_ctx, tier);
                            let _ = handler_event_tx
                                .send(AppEvent::ActiveTierChanged(tier, name))
                                .await;
                        }
                        Err(e) => {
                            let _ = handler_event_tx
                                .send(AppEvent::Error(e.to_string()))
                                .await;
                        }
                    }
                    emit_models_snapshot(&handler_ctx, &handler_event_tx).await;
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

    let backend = ctx.active_inference().await?;
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

    let backend = match ctx.active_inference().await {
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
                let resolved_db = db_name.clone().unwrap_or_else(|| "default".to_string());
                if let Ok(db_path) = ctx.db_path_for(&resolved_db)
                    && let Err(e) = lv_core::sidecar::write_indexed_now(
                        std::path::Path::new(&db_path),
                        env!("CARGO_PKG_VERSION"),
                    ) {
                    tracing::warn!("failed to write sidecar for '{resolved_db}': {e}");
                }
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
    let db_name = ctx.current_db().await;
    let db_path = ctx.db_path_for(&db_name)?;

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

    if let Err(e) = lv_core::sidecar::write_indexed_now(
        std::path::Path::new(&db_path),
        env!("CARGO_PKG_VERSION"),
    ) {
        tracing::warn!("failed to write sidecar for '{db_name}': {e}");
    }
    Ok(())
}

async fn run_serve(config: Config, tier: Option<String>) -> anyhow::Result<()> {
    let ctx: Arc<AppContext> = Arc::new(AppContext::new(config));
    if let Some(ref t) = tier {
        let model_tier = parse_tier(t)?;
        tracing::info!("pre-loading tier '{t}' before MCP server start");
        ctx.load_model(model_tier).await
            .with_context(|| format!("failed to pre-load tier '{t}'"))?;
        ctx.set_active_tier(model_tier).await
            .with_context(|| format!("failed to set active tier '{t}'"))?;
        tracing::info!("tier '{t}' warm and active");
    }
    tracing::info!("MCP server starting on stdio");
    let host: Arc<dyn AppHost> = ctx;
    let server = lv_mcp::VibeMcpServer::new(host);
    lv_mcp::run_stdio(server)
        .await
        .map_err(|e| anyhow::anyhow!("MCP server: {e}"))?;
    tracing::info!("MCP server stopped");
    Ok(())
}

fn parse_tier(s: &str) -> anyhow::Result<ModelTier> {
    s.parse::<ModelTier>()
        .map_err(|()| anyhow::anyhow!("unknown tier '{s}'; use fast, medium, or strong"))
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

async fn run_status(config: Config, json: bool) -> anyhow::Result<()> {
    let ctx = AppContext::new(config);
    let current = ctx.current_db().await;
    let snapshot = collect_declared_status(&ctx, Some(&current)).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        print_status_human(&snapshot);
    }
    Ok(())
}

async fn run_dbs(config: Config, json: bool) -> anyhow::Result<()> {
    let ctx = AppContext::new(config);
    let names = if ctx.config.rag.db_root.is_some() {
        ctx.list_dbs().await?
    } else if ctx.config.rag.db_dir.exists() {
        vec!["default".to_string()]
    } else {
        Vec::new()
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "dbs": names }))?);
    } else if names.is_empty() {
        println!("(no DBs — index something with `lv index` or `/index <path> <name>` in the TUI)");
    } else {
        for n in &names {
            println!("{n}");
        }
    }
    Ok(())
}

async fn run_ls(config: Config, db: &str, limit: usize, json: bool) -> anyhow::Result<()> {
    let ctx = AppContext::new(config);
    let store = ctx.open_store_readonly(db).await?;
    let files = store
        .list_files(limit)
        .await
        .context("list_files failed")?;
    if json {
        println!("{}", serde_json::to_string_pretty(&files)?);
    } else if files.is_empty() {
        println!("(no files in DB '{db}')");
    } else {
        for f in &files {
            let lang = f.language.as_deref().unwrap_or("?");
            println!(
                "[{lang}] {} ({} chunk{})",
                f.file_path,
                f.chunk_count,
                if f.chunk_count == 1 { "" } else { "s" }
            );
        }
    }
    Ok(())
}

fn print_status_human(s: &StatusSnapshot) {
    let ready_glyph = |r: &Readiness| match r {
        Readiness::Ready => "ok".to_string(),
        Readiness::MissingWeights => "missing weights".to_string(),
        Readiness::Unknown => "unknown".to_string(),
    };

    println!("── Models ─────────────────────────────────────");
    let m = &s.models;
    println!("  fast      {} ({}) — {}", m.fast.name, m.fast.backend, ready_glyph(&m.fast.ready));
    println!("  medium    {} ({}) — {}", m.medium.name, m.medium.backend, ready_glyph(&m.medium.ready));
    println!("  strong    {} ({}) — {}", m.strong.name, m.strong.backend, ready_glyph(&m.strong.ready));
    match &m.embedding {
        Some(e) => println!("  embedding {} ({}) — {}", e.name, e.backend, ready_glyph(&e.ready)),
        None => println!("  embedding (not configured — RAG disabled)"),
    }
    match &m.cloud {
        Some(c) => println!("  cloud     {} via {}", c.model, c.provider),
        None => println!("  cloud     (not configured)"),
    }

    println!();
    println!("── Databases ─────────────────────────────────");
    if s.databases.is_empty() {
        println!("  (none)");
    } else {
        for db in &s.databases {
            let marker = if db.is_current { "*" } else { " " };
            let indexed = db.indexed_at.as_deref().unwrap_or("-");
            println!(
                "{marker} {} — {} chunks, {} files, indexed {}",
                db.name, db.total_chunks, db.unique_files, indexed
            );
            if let Some(err) = &db.error {
                println!("    error: {err}");
            }
            if !db.languages.is_empty() {
                let preview: Vec<String> = db.languages
                    .iter()
                    .take(6)
                    .map(|(k, v)| format!("{k}:{v}"))
                    .collect();
                println!("    languages: {}", preview.join(", "));
            }
            println!("    path: {}", db.path.display());
        }
    }

    println!();
    println!("── Runtime ───────────────────────────────────");
    if let Some(root) = &s.db_root {
        println!("  db_root: {}", root.display());
    }
    if let Some(cp) = &s.config_path {
        println!("  config:  {}", cp.display());
    }
    if let Some(r) = &s.runtime {
        let warm_models = if r.warm_models.is_empty() { "(none)".to_string() } else { r.warm_models.join(", ") };
        let warm_dbs = if r.warm_dbs.is_empty() { "(none)".to_string() } else { r.warm_dbs.join(", ") };
        println!("  pid: {}", r.pid);
        println!("  warm models: {warm_models}");
        println!("  warm dbs:    {warm_dbs}");
    }
}

async fn build_model_rows(ctx: &Arc<AppContext>) -> Vec<lv_tui::overlays::ModelRow> {
    use lv_tui::overlays::{LoadState, ModelRow, SlotId};

    let warm: std::collections::HashSet<ModelTier> =
        ctx.warm_tiers().await.into_iter().collect();
    let active = ctx.active_tier().await;
    let cfg = &ctx.config;

    let mut rows: Vec<ModelRow> = Vec::new();
    for (tier, slot) in [
        (ModelTier::Fast, &cfg.models.fast),
        (ModelTier::Medium, &cfg.models.medium),
        (ModelTier::Strong, &cfg.models.strong),
    ] {
        rows.push(ModelRow {
            slot: SlotId::Chat(tier),
            name: slot.name.clone(),
            backend: slot.backend.clone(),
            state: if warm.contains(&tier) { LoadState::Warm } else { LoadState::Cold },
            active: tier == active,
            size_bytes: slot
                .model_path
                .as_deref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .unwrap_or(0),
        });
    }
    if let Some(emb) = cfg.models.embedding.as_ref() {
        rows.push(ModelRow {
            slot: SlotId::Embedding,
            name: emb.name.clone(),
            backend: emb.backend.clone(),
            state: if ctx.is_embedding_warm().await { LoadState::Warm } else { LoadState::Cold },
            active: false,
            size_bytes: emb
                .model_path
                .as_deref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .unwrap_or(0),
        });
    }
    rows
}

async fn emit_models_snapshot(
    ctx: &Arc<AppContext>,
    event_tx: &mpsc::Sender<AppEvent>,
) {
    let rows = build_model_rows(ctx).await;
    let _ = event_tx.send(AppEvent::ModelsSnapshot(rows)).await;
}

async fn emit_warm_count(ctx: &Arc<AppContext>, event_tx: &mpsc::Sender<AppEvent>) {
    let warm = ctx.warm_tiers().await;
    let emb_warm = ctx.is_embedding_warm().await;
    let total = warm.len() + emb_warm as usize;
    let _ = event_tx.send(AppEvent::WarmCountChanged(total, false)).await;
}

fn active_tier_display(ctx: &Arc<AppContext>, tier: ModelTier) -> String {
    let cfg = &ctx.config;
    match tier {
        ModelTier::Fast => cfg.models.fast.name.clone(),
        ModelTier::Medium => cfg.models.medium.name.clone(),
        ModelTier::Strong => cfg.models.strong.name.clone(),
        ModelTier::Cloud => cfg
            .models
            .cloud
            .as_ref()
            .map(|c| c.model.clone())
            .unwrap_or_else(|| "cloud".to_string()),
    }
}
