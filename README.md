# local-vibe (`lv`)

Pure-Rust local coding assistant: chat with a quantized LLM on Metal,
index any directory with on-device ONNX embeddings, search it with
LanceDB, all from one ratatui TUI.

Runs on Apple Silicon (M1–M4). Candle + Metal for inference,
[fastembed-rs](https://crates.io/crates/fastembed) for embeddings,
[LanceDB](https://lancedb.com) for vectors.

---

## Quick start

Assumes `~/.cargo/bin` is on `PATH`, you are on macOS, and you have a
GGUF model supported by Candle (qwen2 / llama family — Qwen 3.5 hybrid
SSM is **not** supported).

```bash
# 1. install the `lv` binary
git clone <this repo> ~/code/local_vibe
cd ~/code/local_vibe
cargo install --path crates/lv-cli

# 2. download a chat model (~4.6 GB)
DEST=~/.lmstudio/models/lmstudio-community/Qwen2.5-7B-Instruct-GGUF
mkdir -p "$DEST"
curl -L -o "$DEST/Qwen2.5-7B-Instruct-Q4_K_M.gguf" \
  https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf
curl -L -o "$DEST/tokenizer.json" \
  https://huggingface.co/Qwen/Qwen2.5-7B-Instruct/resolve/main/tokenizer.json

# 3. write config (macOS path — dirs::config_dir())
mkdir -p ~/Library/Application\ Support/local-vibe
cp local-vibe.example.toml ~/Library/Application\ Support/local-vibe/config.toml
# …edit the paths inside to point at your real GGUF + tokenizer

# 4. run
lv                                 # TUI
lv ask "explain lifetimes in 2 sentences"
```

Inside the TUI, `F1..F5` (or `Ctrl+1..5` where your terminal supports it) jumps between **Chat · Models · Databases · Index · Settings**. Everything else is discoverable by sight. No slash commands to memorise.

First TUI launch takes ~5 s to memory-map the 4.4 GB GGUF and
~10 s extra on first fastembed run (downloads the ONNX embedding
weights into `./.fastembed_cache/`).

---

## How it works

```
 ┌───────────────────────────────────────────────────────────┐
 │ lv-cli (binary)                                           │
 │   main.rs → AppContext (impl AppHost) → dispatcher        │
 │                     │                                     │
 │   ┌─────────────────┼─────────────────┐                   │
 │   ▼                 ▼                 ▼                   │
 │ lv-tui           lv-inference      lv-rag                 │
 │ ratatui UI       fastembed /       LanceDB store +        │
 │ + overlay        mlx-lm            indexer + chunker      │
 │ framework        EmbeddingBackend  + tree-sitter          │
 │                       ▲                ▲                  │
 │                       │                │                  │
 │                   lv-metal            lv-core             │
 │                   Candle+Metal        traits, config,     │
 │                   InferenceBackend    types, status,      │
 │                                       AppHost             │
 │                                                           │
 │                   lv-mcp ◄── Arc<dyn AppHost>             │
 │                   stdio MCP server for Claude Code        │
 └───────────────────────────────────────────────────────────┘
```

Three swappable trait pairs in `lv-core`:

- `InferenceBackend` — streams chat completions. Implementations:
  `MetalBackend` (Candle GGUF, on-device), `MlxLmBackend` (Python HTTP
  fallback).
- `EmbeddingBackend` — produces 384 / 768-d float vectors.
  Implementations: `FastEmbedBackend` (ONNX, pure Rust — default),
  `MlxLmBackend` (HTTP fallback).
- `AppHost` — narrow capability surface that `AppContext` implements;
  MCP and the TUI reach application state through it, which keeps
  `lv-mcp` free of a circular dep on `lv-cli`.

`AppContext` (in `crates/lv-cli/src/app_context.rs`) keeps a per-tier
`HashMap<ModelTier, Arc<dyn InferenceBackend>>` plus an `active_tier`,
so you can load / unload / switch chat models at runtime. Named vector
stores are cached the same way.

---

## Configuration

`lv` reads, in order:

1. `./local-vibe.toml` (current directory)
2. `~/Library/Application Support/local-vibe/config.toml` (macOS) —
   or `~/.config/local-vibe/config.toml` (Linux)

Minimal working config:

```toml
[models.medium]            # chat model
name           = "qwen2.5-7b-instruct"
backend        = "metal"
model_path     = "/Users/YOU/…/Qwen2.5-7B-Instruct-Q4_K_M.gguf"
tokenizer_path = "/Users/YOU/…/tokenizer.json"

[models.embedding]         # omit this section to disable RAG
name    = "bge-small-en"   # or "nomic-embed-text" (768-d)
# backend defaults to "fastembed" — no Python

[rag]
db_root = "/Users/YOU/.local/share/local-vibe/dbs"  # enables multi-DB mode
```

Accepted embedding model names: `bge-small-en` (384-d, ~130 MB),
`bge-base-en` (768-d), `nomic-embed-text-v1.5` (768-d, ~260 MB).

Declare `[models.fast]` and `[models.strong]` the same way if you want
to switch between tiers from inside the TUI (`F2` → Enter on the tier you want).

Omit `db_root` to stay in single-DB mode at `[rag].db_dir`
(default: `~/Library/Application Support/local-vibe/db`).

A full annotated example lives in `local-vibe.example.toml` at the
repo root.

---

## CLI reference

```
lv                    # launch TUI (default)
lv ask "<question>"   # one-shot chat; streams to stdout
lv index <path>       # index a directory into the current DB
lv status             # full snapshot: models + every DB + runtime state
lv status --json      # same, as JSON (for piping into Claude Code etc.)
lv stats              # chunk / file counts in the current DB (legacy)
lv dbs                # list DB names (single line each; --json available)
lv ls <db>            # list files in a DB (--limit N, --json available)
lv models             # print the configured backend for each tier
lv serve              # MCP server on stdio (for Claude Code etc.)
lv --help
```

CLI commands log to stderr. The TUI logs to
`~/.local/share/local-vibe/lv.log` so log lines don't overlap the UI
(tail it with `tail -f ~/.local/share/local-vibe/lv.log`).

---

## TUI reference

The layout borrows from LM Studio: a persistent left sidebar with five
first-class sections, an always-on status strip, and a context-sensitive
hint line at the bottom. There's no command palette — everything is one
`Ctrl+N` jump away.

```
┌ local-vibe ── chat: qwen2.5-7b (medium · warm) · db: rust-rag · 2 warm · idle ─┐
│ F1 Chat       │ ┌─ Chat ───────────────────────┬─ Context ──────────────┐ │
│>F2 Models     │ │ You: …                       │ rust-book.md #3        │ │
│ F3 Databases  │ │ AI:  …                       │   "Spawning Tasks"     │ │
│ F4 Index      │ │                              │                        │ │
│ F5 Settings   │ │ > _                          │                        │ │
│               │ └──────────────────────────────┴────────────────────────┘ │
│ ?: help       │  Enter send · Tab → Context · ↑↓ scroll · F1..F5 sect. │
└───────────────┴──────────────────────────────────────────────────────────────┘
```

### Global keys

| Key                                | Effect                                                     |
| ---------------------------------- | ---------------------------------------------------------- |
| **Ctrl+1 … Ctrl+5**                | jump to Chat · Models · Databases · Index · Settings       |
| **Tab**                            | cycle focus between sub-panes of the current section       |
| **Esc**                            | back out of a focused sub-pane or peek overlay             |
| **?** (when not typing)            | toggle the help overlay                                    |
| **Ctrl-C / Ctrl-Q**                | quit                                                       |

### F1 · Chat

Two-column layout, always. Left (~70%) is the conversation + input;
right (~30%) is the Context pane showing retrieved chunks for the last
answer. `Tab` toggles focus input ↔ context. `Enter` sends. `↑/↓`
scroll the history (input focus) or move a cursor over chunks (context
focus). Typing `/anything` (except `/quit`) is passed to the model as
prose — no special slash handling.

### F2 · Models

One row per slot: `fast · medium · strong · cloud · embed`. Columns
show name, backend, warm/cold state, and an active marker.

| Key on a selected row | Effect                                               |
| --------------------- | ---------------------------------------------------- |
| **Enter** on cold     | load the tier and make it active for chat            |
| **Enter** on warm     | make it active without re-loading                    |
| **l**                 | load (but don't change active tier)                  |
| **u**                 | unload (refused on the currently active tier)        |
| **a**                 | set active — requires the tier to already be warm    |

### F3 · Databases

Two columns. Left: every DB with an active marker. Right: detail for
the selected DB — path, indexed-at timestamp, file and chunk counts,
top-5 language histogram, last error if any.

| Key                                | Effect                                                     |
| ---------------------------------- | ---------------------------------------------------------- |
| **↑ / ↓**                          | select a DB                                                |
| **Enter**                          | activate (and jump back to Chat)                           |
| **b**                              | file browser peek (language pills `1…9`, `0` clears)       |

### F4 · Index

Two text fields stacked: **Path** and **Into**. Entering the section
prefills Into with the active DB. `Tab` inside Path runs filesystem
completion; falling through, it cycles focus. `Enter` submits. While
indexing, a magenta progress bar shows `done/total` and the current
file. `↑/↓` cycles between fields.

### F5 · Settings

Read-only: version, config path, DB root, process id, warm models and
DBs, session id. Right panel has a compact global + per-section keybind
reference. Not editable in this version — config changes are still a
TOML edit + restart.

### Status strip

Dot-separated segments at the top of every screen:

```
 ◆ local-vibe · medium:qwen2.5-7b · db:rust-rag · 52 files · 2 warm
```

The active model turns yellow during load and green once warm. `N warm`
counts every tier held in memory including the embedder. A magenta
`indexing done/total: file` segment appears while an index run is in
flight.

---

## Use as an MCP server

`lv serve` speaks MCP over stdio, so any MCP client (Claude Code, Cursor,
custom agents) can call into the local index. Five tools are exposed;
the DB-specific ones accept an optional `db` argument that defaults to
the server's current DB.

| Tool              | What it does                                                   |
| ----------------- | -------------------------------------------------------------- |
| `search_code`     | semantic search; filters by language / file_path / `db`        |
| `index_directory` | parse + chunk + embed a directory into the store (or `db`)     |
| `get_stats`       | total chunks and unique files, optionally per `db`             |
| `list_sources`    | summary of indexed files, optionally per `db`                  |
| `get_status`      | full snapshot JSON: models, every DB, runtime state            |

Wire it into Claude Code:

```bash
claude mcp add lv lv serve
```

The server uses the *current* DB (whichever `F3` → Enter would pick in
the TUI) when no `db` argument is given. Logs go to
`~/.local/share/local-vibe/lv-mcp.log` so they don't corrupt the
JSON-RPC frames on stdout.

---

## Per-DB metadata sidecar

Every successful index writes a `.lv-meta.toml` next to the LanceDB
directory with the RFC3339 `indexed_at` timestamp and the `lv` version
that wrote it. Missing or malformed sidecars are never fatal — they
just render as `-` in `lv status` / the Databases section.

---

## Project layout

```
crates/
 ├─ lv-core        shared traits, config, types, errors,
 │                 status snapshot, sidecar helpers, AppHost trait
 ├─ lv-inference   EmbeddingBackend impls (FastEmbed, MlxLm)
 ├─ lv-metal       Candle + Metal InferenceBackend (GGUF)
 ├─ lv-rag         LanceDB store, indexer, chunker, parsers, RRF
 ├─ lv-tui         ratatui sections (Chat/Models/DBs/Index/Settings)
 ├─ lv-mcp         stdio MCP server backed by AppHost
 └─ lv-cli         binary; wires everything together
```

---

## Status and known gaps

Working end-to-end today:

- Chat via Metal (qwen2 / llama GGUF, ChatML + Gemma templates auto-detected)
- Five-section sidebar TUI with always-on status strip and contextual hint line
- Runtime model load / unload / tier switching from the Models section
- Embeddings via fastembed (pure-Rust ONNX; no Python)
- Single-DB and multi-DB RAG via `rag.db_root`
- Databases section with per-DB detail + file-browser peek (language pills)
- Index section with filesystem Tab completion + live progress bar
- `lv status` / MCP `get_status` — unified snapshot across every DB

Known gaps / rough edges:

- **Qwen 3.5 hybrid SSM** — Candle has no backend for `general.architecture = "qwen35"`; use Qwen 2.5 or Llama / Gemma for now.
- **Embedding unload** — the embedding row in Models is display-only; embedding uses a separate lazy-init path.
- **Cloud tier** — the config slot exists but loading `ModelTier::Cloud` is not wired up; use local tiers for now.
- **`fastembed` cache is cwd-relative** (fastembed 5.x default). Gitignore
  `./.fastembed_cache/` or plan to pin a global cache dir.
- **Config discovery** is platform-dependent (see the two paths above); a
  follow-up will also check `~/.config/local-vibe/config.toml` on macOS.
- **Recent-runs history** on the Index screen is planned; for now only the
  live progress is shown during a run.
- **Conversation history** (persisted chats) not yet wired into the Chat section.

---

## Development

```bash
cargo check   --workspace
cargo test    --workspace              # all green
cargo clippy  --workspace --all-targets -- -D warnings
```

Reinstall after changes:

```bash
cargo install --path crates/lv-cli --force
```

Design specs for recent work live in `docs/superpowers/specs/`.
