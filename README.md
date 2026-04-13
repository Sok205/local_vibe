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

First TUI launch takes ~5 s to memory-map the 4.4 GB GGUF and
~10 s extra on first fastembed run (downloads the ONNX embedding
weights into `./.fastembed_cache/`).

---

## How it works

```
 ┌───────────────────────────────────────────────────────────┐
 │ lv-cli (binary)                                           │
 │   main.rs → AppContext → dispatcher                       │
 │                     │                                     │
 │   ┌─────────────────┼─────────────────┐                   │
 │   ▼                 ▼                 ▼                   │
 │ lv-tui           lv-inference      lv-rag                 │
 │ ratatui UI       fastembed /       LanceDB store +        │
 │ + slash          mlx-lm            indexer + chunker      │
 │ commands         EmbeddingBackend  + tree-sitter          │
 │                       ▲                ▲                  │
 │                       │                │                  │
 │                   lv-metal            lv-core             │
 │                   Candle+Metal        traits, config,     │
 │                   InferenceBackend    types, errors       │
 └───────────────────────────────────────────────────────────┘
```

Two swappable trait pairs in `lv-core`:
- `InferenceBackend` — streams chat completions. Implementations:
  `MetalBackend` (Candle GGUF, on-device), `MlxLmBackend` (Python HTTP
  fallback).
- `EmbeddingBackend` — produces 384 / 768-d float vectors.
  Implementations: `FastEmbedBackend` (ONNX, pure Rust — default),
  `MlxLmBackend` (HTTP fallback).

`AppContext` (in `crates/lv-cli/src/app_context.rs`) lazily builds each
backend on first use and caches named vector stores so `/db <name>`
switches cost nothing after the first open.

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
lv stats              # chunk / file counts in the current DB
lv models             # print the configured backend for each tier
lv serve              # MCP server on stdio (for Claude Code etc.)
lv --help
```

CLI commands log to stderr. The TUI logs to
`~/.local/share/local-vibe/lv.log` so log lines don't overlap the UI
(tail it with `tail -f ~/.local/share/local-vibe/lv.log`).

---

## TUI reference

Inside the TUI:

| Input                              | Effect                             |
| ---------------------------------- | ---------------------------------- |
| any plain text                     | chat with the model                |
| `/dbs`                             | list named vector stores           |
| `/db <name>`                       | switch the current DB              |
| `/index <path>`                    | index into the current DB          |
| `/index <path> <name>`             | index into a named DB              |
| `/quit`                            | exit cleanly                       |
| **Enter**                          | submit                             |
| **↑ / ↓**                          | scroll chat                        |
| **Tab**                            | toggle the right-hand context pane |
| **Ctrl-C** / **Ctrl-Q**            | quit                               |

Status bar shows `[tier: model]  [db: name]  [N indexed]` plus a live
`[indexing done/total: file]` segment while `/index` is running.

---

## Use as an MCP server

`lv serve` speaks MCP over stdio, so any MCP client (Claude Code, Cursor,
custom agents) can call into the local index. Four tools are exposed:

| Tool              | What it does                                         |
| ----------------- | ---------------------------------------------------- |
| `search_code`     | semantic search; filters by language / file_path     |
| `index_directory` | parse + chunk + embed a directory into the store     |
| `get_stats`       | total chunks and unique files                        |
| `list_sources`    | summary of indexed files                             |

Wire it into Claude Code:

```bash
claude mcp add lv lv serve
```

The server uses the *current* DB (whichever `/db <name>` would pick in the
TUI). Logs go to `~/.local/share/local-vibe/lv-mcp.log` so they don't
corrupt the JSON-RPC frames on stdout.

---

## Project layout

```
crates/
 ├─ lv-core        shared traits, config, types, errors
 ├─ lv-inference   EmbeddingBackend impls (FastEmbed, MlxLm)
 ├─ lv-metal       Candle + Metal InferenceBackend (GGUF)
 ├─ lv-rag         LanceDB store, indexer, chunker, parsers, RRF
 ├─ lv-tui         ratatui widgets + slash-command parser
 └─ lv-cli         binary; wires everything together
```

---

## Status and known gaps

Working end-to-end today:

- Chat via Metal (qwen2 / llama GGUF, ChatML + Gemma templates auto-detected)
- Embeddings via fastembed (pure-Rust ONNX; no Python)
- Single-DB and multi-DB RAG via `rag.db_root`
- TUI with live indexing progress and DB switching

Known gaps / rough edges:

- **Qwen 3.5 hybrid SSM** — Candle has no backend for `general.architecture = "qwen35"`; use Qwen 2.5 or Llama / Gemma for now.
- **`fastembed` cache is cwd-relative** (fastembed 5.x default). Gitignore
  `./.fastembed_cache/` or plan to pin a global cache dir.
- **Config discovery** is platform-dependent (see the two paths above); a
  follow-up will also check `~/.config/local-vibe/config.toml` on macOS.
- **Stats on `lv stats`** reflects the *current* DB only; there is no
  command to print stats for every DB at once.

---

## Development

```bash
cargo check   --workspace
cargo test    --workspace              # 51 tests, all green
cargo clippy  --workspace --all-targets -- -D warnings
```

Reinstall after changes:

```bash
cargo install --path crates/lv-cli --force
```
