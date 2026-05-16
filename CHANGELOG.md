# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Dual MIT / Apache-2.0 license
- CI workflow (fmt + clippy `-D warnings` + test on macOS and Linux)
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`
- Crate metadata (description, repository, keywords) on every crate
- Crate-level `//!` docs on every `lib.rs`
- `localvibe` as the canonical binary name; `lv` kept as a back-compat alias

### Changed
- Moved off the personal candle fork onto upstream `huggingface/candle`
  (huggingface/candle#3536 added the `clear_kv_cache` parity we needed
  for quantized Llama and Qwen2)
- `lv-metal` upgraded from edition 2021 to 2024
- LanceDB column decoding in `lv-rag::store` now returns `VibeError::Store`
  instead of panicking on a missing column

### Removed
- Unused `rand` dependency from `lv-metal`
- Unused `rayon` dependency from `lv-rag`
- Two `#[allow(dead_code)]` annotations (the third is kept with a comment
  explaining the rmcp macro that needs it)
- Internal copyrighted-content corpora from the repo and git history

[Unreleased]: https://github.com/Sok205/local_vibe/commits/main
