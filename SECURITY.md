# Security Policy

## Reporting a vulnerability

If you believe you've found a security issue in LocalVibe, **please do not open
a public issue.** Instead, report it privately via GitHub Security Advisories:

<https://github.com/Sok205/local_vibe/security/advisories/new>

Include:
- A description of the issue and its impact
- Steps to reproduce (a minimal proof-of-concept is ideal)
- The affected version / commit SHA

You can expect an initial acknowledgement within 7 days. Fixes for confirmed
issues will land on `main` and be called out in the changelog once published.

## Scope

LocalVibe runs entirely on the user's machine and binds by default to
`127.0.0.1`. Issues worth reporting include, but aren't limited to:
- Path traversal in the indexer or HTTP/MCP surfaces
- Command injection via filenames, model paths, or configuration
- Memory safety bugs in `lv-metal` or other native-FFI code
- Auth-bypass on the OpenAI-compatible HTTP server

Out of scope:
- Vulnerabilities that require running an attacker-controlled binary
- Issues in third-party model files or downloaded weights
- DoS via locally crafted inputs to a CLI invoked by the same user
