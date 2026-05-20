# Release Config Compatibility Audit

## Scope

- User config: `~/.yode/config.toml`
- Managed config: `~/.yode/managed-config.toml`
- Project config: `.yode/config.toml`
- Local override: `.yode/config.local.toml`
- Built-in defaults: `config/default.toml`

## Compatibility Checks

- Older config files without the `[update]` section still deserialize with update checks/downloads enabled by default.
- Default merging preserves explicit user values while adding missing default keys.
- Remote MCP server configs can use URL transports without a command field.
- Strict permission mode keeps legacy behavior: mutating tools confirm, read-only tools allow.
- README continues to document managed, user, project, local, session, and CLI config layers.

## Verification

- `cargo test -q -p yode-core config`
- `cargo test -q -p yode-core permission`
- `rg -q '~/.yode/managed-config.toml' README.md`
- `rg -q '.yode/config.local.toml' README.md`

## Residual Risk

- This audit confirms deserialize/default-merge compatibility and documented scope precedence, but it does not exercise every TUI command that writes `.yode/config.toml`.
- Managed/user/project/local merge precedence remains covered by permission source tests and diagnostics, not by a single end-to-end config file fixture.
