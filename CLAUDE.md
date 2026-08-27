# CLAUDE.md

This repository is **Desktop GUI-first**. Read and follow `AGENTS.md` as the canonical development guidance.

## Non-negotiable product direction

Yode has retired its root CLI/TUI product.

- `apps/yode-desktop/` is the only supported user-facing product entry point.
- Do not recreate `src/main.rs`, a root Cargo package, TUI, `clap` command trees, shell completions, or CLI-only setup/update flows.
- Implement reusable Agent capabilities in Rust crates first, then expose them through the Tauri/Desktop runtime.
- Shell tools used internally by the Agent are runtime capabilities and must not be turned back into a CLI product surface.
- When old docs/tests/scripts reference CLI behavior, migrate them toward Desktop/runtime behavior instead of adding compatibility code.

## Required validation target

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo check --workspace --all-targets
cargo test --workspace
cd apps/yode-desktop && pnpm test && pnpm build
```

For architecture, priorities, repository layout, and detailed conventions, use `AGENTS.md`.
