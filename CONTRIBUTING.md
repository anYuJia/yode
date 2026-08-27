# Contributing

Yode is a **Desktop GUI-first** Rust/Tauri workspace. Keep changes scoped, tested, and explicit about user-facing behavior.

## Product boundary

- `apps/yode-desktop/` is the supported user-facing product.
- Do not recreate the retired root CLI/TUI product, root `src/main.rs`, `clap` command tree, shell completions, or `cargo install --path .` workflow.
- New Agent capabilities should live in reusable Rust crates first and be exposed to the Desktop app through Tauri commands/events.
- Shell, terminal, bash, and PowerShell functionality used by the Agent are runtime tools, not a reason to restore a CLI product surface.
- Migrate active scripts/docs/tests away from retired CLI assumptions instead of adding compatibility code.

## Development

Run the narrowest relevant checks while developing, then target the full gates for changes landing on `main`:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo check --workspace --all-targets
cargo test --workspace
```

For Desktop changes:

```bash
cd apps/yode-desktop
pnpm install --frozen-lockfile
pnpm test
pnpm build
```

Provider integration tests belong to `yode-llm`:

```bash
cargo test -p yode-llm --test anthropic_integration
```

## Engineering expectations

- Prefer small commits that isolate one behavior change.
- Do not commit secrets, provider API keys, or local `.yode` runtime artifacts.
- Preserve cancellation, timeout, recovery, artifact, and fail-closed behavior when changing runtime code.
- Code-changing Agent flows should produce test/review/verification evidence rather than relying on a model's success claim.
- New runtime state should be structured where practical so Desktop RunInspector, recovery, telemetry, and YodeBench can consume it.

## Pull Requests

Include a short summary, validation commands, and any compatibility or security impact. If a change affects providers, tools, MCP, browser/computer-use, updates, permissions, sandboxing, verification, scheduling, or recovery, include regression coverage for the changed behavior.
