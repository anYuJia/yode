# Contributing

Yode is a Rust workspace. Keep changes scoped, tested, and explicit about user-facing behavior.

## Development

- Run `cargo check --workspace` before sending a PR.
- Run the narrowest relevant `cargo test -p <crate>` commands for changed crates.
- Do not commit secrets, provider API keys, or local `.yode` runtime artifacts.
- Prefer small patches that isolate behavior changes from formatting-only churn.

## Pull Requests

Include a short summary, validation commands, and any compatibility or security impact. If a change affects provider behavior, tools, MCP, updates, or permissions, include at least one regression test.
