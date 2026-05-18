# Four-Month Claude Code Rev Optimization Report

## 中文导读

这是一份给后续 AI 直接执行的 4 个月优化报告。报告基于当前 `yode` 代码库、本地 `claude-code-rev` 参考实现、以及既有 parity 文档，重点不是继续堆命令数量，而是把长上下文、工具安全、子代理、远程任务、MCP/skills/plugins、诊断和 CI 合约做成可持续演进的产品级能力。

执行方式：按 16 周路线图推进，每周 2 到 5 个小提交；每个任务都要先读列出的入口文件，再做窄改动、跑对应验证命令、最后独立提交。除非用户明确要求，不要提交 `.serena/project.yml`。

## Purpose

This report turns the current `yode` versus `claude-code-rev` gap into a four-month execution plan that another AI agent can follow without rediscovering the whole repository.

Reference source:

- Yode workspace: `/Users/pyu/code/yode`
- Claude reference: `/Users/pyu/code/claude/claude-code-rev`
- Current branch state when this report was written: `main` is ahead of `origin/main`; only `.serena/project.yml` was dirty and should not be included in feature commits.

The goal is not to clone Claude Code line by line. The goal is to reach comparable coding-agent quality in the areas that affect real sessions:

- long-session context survival
- tool and permission correctness
- subagent, task, and remote continuation
- MCP, skills, plugins, and configuration governance
- operator-visible diagnostics
- replayable parity validation

## Executive Summary

Yode already has a strong Rust-native runtime: tool execution, permissions, hooks, MCP resources, skills, background tasks, review pipelines, remote-control artifacts, session memory, checkpoint/rewind, compact restore blocks, and a large parity CI/script surface.

The remaining work is less about raw feature count and more about production polish:

1. Context management should become budgeted, replayable, and less dependent on a single compaction strategy.
2. Subagents should support Claude-style forked context and worktree isolation with stronger cache and permission semantics.
3. Permission and shell safety should move from pattern checks to structured command semantics and persistent rule UX.
4. Remote, task, and artifact flows should have durable event-log replay and resumable state machines.
5. MCP, skills, and plugins should converge into a unified extension system with trust, activation, diagnostics, and hot reload.
6. CI should stop proving only that outputs exist and start proving interaction contracts stay stable.

This is a four-month workload for one strong AI engineering lane plus occasional human review, assuming small commits and continuous verification.

## Current Yode Strengths

Important existing anchors:

- Tool registry and built-ins: `crates/yode-tools/src/builtin/mod.rs`
- Engine runtime split: `crates/yode-core/src/engine/*`
- Context manager and compaction: `crates/yode-core/src/context_manager/*`, `crates/yode-core/src/engine/compaction_runtime.rs`
- Permission governance: `crates/yode-core/src/permission/*`
- TUI command surface: `crates/yode-tui/src/commands/*`
- Runtime observability: `crates/yode-tui/src/runtime_*`, `crates/yode-tui/src/commands/info/*`
- Artifact and parity scripts: `scripts/parity-*`, `.github/workflows/ci.yml`

Recent progress already moved Yode closer to Claude behavior:

- effective context window reserves output headroom
- compact start events are visible
- post-compact pressure is surfaced as signed delta
- preserved read-file tail avoids duplicate restore snippets
- partial compact prompts are direction-aware
- `/context` gives operator suggestions

## High-Level Gap Map

| Area | Current Yode State | Claude Reference | Remaining Gap | Priority |
| --- | --- | --- | --- | --- |
| Context and compaction | Strong manual/auto/reactive compact, restore artifacts, memory snapshots | `services/compact/*`, `services/contextCollapse/*` | Missing granular context-collapse lane, stricter token budgets, persisted compact boundaries as first-class transcript events | P0 |
| Post-compact restoration | Restores runtime/files/skills/MCP/cache state | `createPostCompactFileAttachments`, plan/skills/async-agent attachments | Need global token budget accounting across all restore blocks and explicit plan/async-agent restoration contracts | P0 |
| Subagents | Background agents, teams, task output, hooks | `AgentTool/forkSubagent.ts`, built-in agent definitions | Missing implicit fork mode, cache-identical placeholder protocol, worktree fork notices, permission bubbling | P0 |
| Permissions | Modes, rules, denial clustering, shell prefix suggestions | `tools/BashTool/*`, `utils/permissions/*` | Bash/PowerShell semantics are still mostly regex/prefix based; need structured read-only/destructive parser and rule editor UX | P0 |
| Commands | Broad command surface with diagnostics | `src/commands/*` | Missing product commands or equivalents: plugin, branch, add-dir, files, tag, stats/usage, keybindings, IDE, install/login variants | P1 |
| MCP | Resource tools, auth, cleanup, diagnostics | `services/mcp/*`, `commands/mcp/*` | Needs official-registry style inventory, elicitation validation, channel permissions, hot reload, trust UX | P1 |
| Skills/plugins | Skills exist and are path-gated | `services/skillSearch/*`, `services/plugins/*`, `commands/plugin/*` | No full plugin marketplace/trust lifecycle; skill invocation and compaction budget need stronger persistence | P1 |
| Remote/runtime tasks | Remote queue/control artifacts exist | `bridge/*`, `remote/*`, `QueryEngine.ts` | Needs durable event-log replay, remote storage backend, reconnect state machine, status protocol parity | P1 |
| Session history | SQLite/session memory/checkpoints | `utils/sessionStorage.ts`, compact boundary JSONL | Need compact boundary records and transcript replay as a product primitive | P1 |
| UI/TUI ergonomics | Good text diagnostics and inspector | React/Ink components with rich pickers | Need more interactive pickers, command panels, keyboard config, context visualization density | P2 |
| CI and parity | Many parity scripts and workflows | Reference has less repo-level CI but richer runtime contracts | Need fewer script fragments, stronger golden fixtures, scenario replay, command contract tests | P0 |

## Four-Month Roadmap

The plan is 16 weeks. Each week should produce 2 to 5 small commits. Never mix `.serena/project.yml` or local generated drift into feature commits.

Default validation after each commit:

```bash
cargo fmt -p yode -p yode-core -p yode-llm -p yode-tools -p yode-tui -p yode-mcp -p yode-agent -- --check
cargo check -q
git diff --check
```

Use broader validation at sprint boundaries:

```bash
cargo test --workspace --lib
bash scripts/parity-snapshot-ci.sh
bash scripts/parity-replay-ci.sh
bash scripts/parity-docs-ci.sh
bash scripts/benchmark-snapshot.sh
```

## Month 1: Context Survival And Transcript Contracts

### Week 1: Compact Boundary As First-Class Session Event

Objective: make compaction visible in storage, replay, and UI as a stable boundary, not only a system block.

Reference:

- Claude: `src/utils/messages.ts`, `src/QueryEngine.ts`, `src/utils/sessionStorage.ts`
- Yode: `crates/yode-core/src/engine/types.rs`, `crates/yode-core/src/db/*`, `crates/yode-core/src/transcript/*`, `crates/yode-core/src/engine/compaction_runtime.rs`

Tasks:

- Add a typed compact-boundary record to transcript/session storage.
- Include mode, removed count, preserved tail range, summary fingerprint, post-compact token estimate, and artifact paths.
- Teach replay/render paths to preserve compact boundaries.
- Add `/memory transcripts` or `/inspect artifact` display for compact boundaries.

Acceptance:

- A forced compact creates both restore system blocks and a compact-boundary record.
- Session resume can display the latest boundary without parsing human prose.
- Targeted tests cover full, partial up_to, partial from, and reactive compact boundaries.

Validation:

```bash
cargo test -q -p yode-core compaction
cargo test -q -p yode-core transcript
cargo test -q -p yode-tui memory
```

### Week 2: Restore Budget Manager

Objective: replace independent restore snippets with a shared budget planner.

Reference:

- Claude: `POST_COMPACT_TOKEN_BUDGET`, `POST_COMPACT_MAX_TOKENS_PER_FILE`, `POST_COMPACT_SKILLS_TOKEN_BUDGET` in `src/services/compact/compact.ts`
- Yode: `build_post_compact_restore_messages` in `crates/yode-core/src/engine/compaction_runtime.rs`

Tasks:

- Introduce `RestoreBudget` with total budget, per-block caps, and truncation reasons.
- Budget runtime, files, skills, MCP, cache, plan, tasks, and memory blocks together.
- Emit a budget table into restore artifacts.
- Show budget pressure in `/context`.

Acceptance:

- Restore blocks never exceed configured total budget in tests.
- If a file/skill is truncated, the block says exactly how to recover the full content.
- `/context` shows restore budget used/limit after compact.

Validation:

```bash
cargo test -q -p yode-core compaction
cargo test -q -p yode-tui context_cmd
bash scripts/verify-compact-artifacts.sh
```

### Week 3: Plan And Async-Agent Restoration

Objective: preserve active plans and running background agents across compact.

Reference:

- Claude: `createPlanAttachmentIfNeeded`, `createPlanModeAttachmentIfNeeded`, `createAsyncAgentAttachmentsIfNeeded`
- Yode: `crates/yode-tools/src/runtime_tasks/*`, `crates/yode-core/src/engine/subagent_runner.rs`, `crates/yode-tools/src/builtin/plan_mode/*`

Tasks:

- Add restore block kinds for plan mode, active plan file, and async task status.
- Include task id, status, output path, last progress, and retrieval state.
- Teach subagent result notifications not to be lost across compact.
- Add TUI display in `/tasks follow latest` and `/context`.

Acceptance:

- Compact during a running background agent shows a task restore block.
- Plan mode remains clear after compact.
- Follow-up turns do not spawn duplicate workers because of lost task context.

Validation:

```bash
cargo test -q -p yode-core compaction
cargo test -q -p yode-tools runtime_tasks
cargo test -q -p yode-tui tasks
```

### Week 4: Context Collapse Prototype

Objective: add a second path beside compaction: summarize old tool-heavy spans without collapsing the whole transcript.

Reference:

- Claude: `src/services/contextCollapse/index.ts`, `operations.ts`, `persist.ts`
- Yode: `crates/yode-core/src/context_manager/runtime/compression.rs`, `crates/yode-core/src/engine/compaction_runtime.rs`

Tasks:

- Add `context_collapse` module behind config flag.
- Persist collapse operations as artifacts with source message ranges and replacement summaries.
- Collapse low-value old tool output before auto compact triggers.
- Add `/context collapse status` or extend `/context`.

Acceptance:

- Long tool-output sessions reduce token pressure without full compact.
- Collapse operations are replayable and reversible in tests.
- Auto compact threshold is reached later in benchmark snapshot.

Validation:

```bash
cargo test -q -p yode-core context_manager
cargo test -q -p yode-core compaction
bash scripts/benchmark-snapshot.sh
```

Progress:

- 2026-05-16: Added a config-gated (`YODE_CONTEXT_COLLAPSE=1`) context collapse prototype that summarizes old large tool outputs without removing messages, persists reversible JSON operations under `.yode/context-collapse`, and surfaces collapse status in `/context`. Verified with `cargo test -q -p yode-core context_manager`, `cargo test -q -p yode-core compaction`, `cargo test -q -p yode-core context_collapse`, `bash scripts/benchmark-snapshot.sh`, `cargo check -q`, and `git diff --check`. Remaining risk: collapse summaries are deterministic local previews rather than LLM-generated semantic summaries, and auto-collapse is env-gated until product config UX lands.

Month 1 milestone:

- Long sessions have typed compact/collapse boundaries.
- Restore context has shared budget accounting.
- Plan/task state survives compact.

## Month 2: Tool Execution, Permissions, And Subagent Parity

### Week 5: Structured Bash And PowerShell Semantics

Objective: move safety decisions from broad regex to structured command semantics.

Reference:

- Claude: `tools/BashTool/commandSemantics.ts`, `bashSecurity.ts`, `readOnlyValidation.ts`, `sedValidation.ts`
- Yode: `crates/yode-tools/src/builtin/bash/*`, `crates/yode-tools/src/builtin/powershell/*`, `crates/yode-core/src/permission/shell.rs`

Tasks:

- Add a command semantic classifier with categories: read-only, package install, network, git mutating, destructive, interactive, unknown.
- Detect chained commands and report the highest risk segment.
- Port safe read-only prefix rules into structured tests.
- Add explain output to `/permissions`.

Acceptance:

- `git status && rg foo` is read-only.
- `git reset --hard` and `rm -rf /tmp/project` are blocked or require explicit dangerous confirmation.
- Sed/awk file edits are redirected toward `edit_file`.

Validation:

```bash
cargo test -q -p yode-core permission
cargo test -q -p yode-tools bash
cargo test -q -p yode-tools powershell
```

Progress:

- 2026-05-16: Added structured bash command semantic analysis in permission classification with read-only/package-install/network/git-mutating/destructive/interactive/unknown categories, highest-risk chained segment reporting, `/permissions explain` semantic output, and focused tests for read-only chains, destructive `rm -rf /tmp/project`/`git reset --hard`, and sed/awk edit redirection guidance. Verified with `cargo test -q -p yode-core permission`, `cargo test -q -p yode-tools bash`, `cargo test -q -p yode-tools powershell`, `cargo check -q`, and `git diff --check`. Remaining risk: bash/powershell tool-local guards still maintain their own parsers and should be unified with this semantic model in a later hardening pass.

### Week 6: Permission Rule Editor And Persistence UX

Objective: make permission tuning product-grade.

Reference:

- Claude: `commands/permissions/*`, `utils/permissions/*`
- Yode: `crates/yode-tui/src/commands/tools/permissions.rs`, `crates/yode-core/src/config.rs`, `crates/yode-core/src/permission/config.rs`

Tasks:

- Add `/permissions add`, `/permissions remove`, `/permissions sources`, `/permissions explain <tool> <content>`.
- Write rules to project or user config explicitly, never silently.
- Include dry-run output before writing.
- Add conflict detection across managed/user/project/session sources.

Acceptance:

- Repeated confirmation suggestions can be turned into a rule with one command.
- Managed deny cannot be overridden by user allow.
- `/permissions sources` shows precedence and file paths.

Validation:

```bash
cargo test -q -p yode-core permission
cargo test -q -p yode-tui permissions
```

Progress:

- 2026-05-16: Added `/permissions sources` as a precedence-oriented source view with file paths and conflict lines, plus core conflict detection that shows higher-precedence rules overriding lower-precedence rules. Added focused coverage that managed deny wins over user allow and that sources output includes precedence, paths, and conflicts. Verified with `cargo test -q -p yode-core permission`, `cargo test -q -p yode-tui permissions`, `cargo check -q`, and `git diff --check`. Remaining risk: `/permissions add/remove` persistence UX is still pending.
- 2026-05-16: Added `/permissions add` and `/permissions remove` dry-run-first commands for explicit `user`/`project` scopes, requiring `--write` before updating `.yode/config.toml` or `~/.yode/config.toml`. Writes update the current runtime permission manager and TOML `permissions.always_allow`/`always_deny`/`always_ask` buckets without duplicating rules. Verified with `cargo test -q -p yode-core permission`, `cargo test -q -p yode-tui permissions`, `cargo check -q`, and `git diff --check`. Remaining risk: config reload/source-view refresh should be tightened so newly written rules appear in `/permissions sources` without relying on runtime rule snapshots.

### Week 7: Forked Subagent Mode

Objective: support Claude-style implicit fork workers with inherited context and cache-friendly placeholder protocol.

Reference:

- Claude: `src/tools/AgentTool/forkSubagent.ts`, `src/tools/AgentTool/runAgent.ts`
- Yode: `crates/yode-tools/src/builtin/agent/mod.rs`, `crates/yode-core/src/engine/subagent_runner.rs`, `crates/yode-tools/src/builtin/team_runtime/mod.rs`

Tasks:

- Add `fork_context` or implicit fork option to `agent`.
- Build fork child messages from parent history with placeholder tool results.
- Prevent recursive fork spawning.
- Add worktree notice injection for isolated workers.
- Track cache stability via prompt fingerprint.

Acceptance:

- Two forked agents share identical prefix except final directive.
- Fork child cannot spawn more fork children.
- Worktree fork receives path translation notice.

Validation:

```bash
cargo test -q -p yode-core subagent
cargo test -q -p yode-tools agent
cargo test -q -p yode-tools team_runtime
```

Progress:

- 2026-05-16: Added a `fork_context` agent option and subagent runner fork boilerplate with a recursive fork guard, stable directive prefix/fingerprint, and worktree path-translation notice. Added focused coverage that fork child prompts share a byte-identical prefix before the directive, worktree forks receive path translation guidance, and the agent tool forwards `fork_context`. Verified with `cargo test -q -p yode-core subagent`, `cargo test -q -p yode-tools agent`, `cargo test -q -p yode-tools team_runtime`, `cargo check -q`, and `git diff --check`. Remaining risk: fork children still receive a directive wrapper rather than full parent transcript messages with placeholder tool results; that history plumbing remains the next hardening step.

### Week 8: Tool Result Canonicalization And UI Grouping

Objective: make tool output reviewable and compact-friendly.

Reference:

- Claude: `utils/groupToolUses.ts`, `components/Message.tsx`, `services/toolUseSummary/*`
- Yode: `crates/yode-core/src/engine/tool_result.rs`, `crates/yode-tui/src/tool_grouping.rs`, `crates/yode-tui/src/tool_output_summary.rs`

Tasks:

- Normalize tool result summaries for file reads, edits, shell, web, MCP, and agents.
- Add stable grouping ids for concurrent tool calls.
- Persist compact representations separately from full artifacts.
- Add output-size warnings and exact re-read instructions.

Acceptance:

- Replaying the same tool event stream renders stable grouped output.
- Large results are summarized with artifact path and re-run guidance.
- Compact/collapse can use compact representations without losing full artifact.

Validation:

```bash
cargo test -q -p yode-core runtime
cargo test -q -p yode-tui tool_grouping
cargo test -q -p yode-tui tool_output_summary
bash scripts/parity-replay-ci.sh
```

Progress:

- 2026-05-16: Added stable grouped-tool batch ids derived from replay-stable tool call ids, names, arguments, and item kinds, plus runtime truncation warnings in tool result summaries with explicit narrower-query/offset-limit guidance. Verified with `cargo test -q -p yode-core runtime`, `cargo test -q -p yode-tui tool_grouping`, `cargo test -q -p yode-tui tool_output_summary`, `bash scripts/parity-replay-ci.sh`, `cargo check -q`, and `git diff --check`. Remaining risk: compact representations are surfaced through runtime metadata and UI summaries, but full separate persisted compact-result artifacts are still pending.

Month 2 milestone:

- Shell safety is explainable and semantically tested.
- Subagents can fork context safely.
- Tool output is stable enough for replay and compaction.

## Month 3: Extension System, Remote Runtime, And Product Commands

### Week 9: MCP Registry, Trust, And Hot Reload

Objective: bring MCP management closer to Claude's product surface.

Reference:

- Claude: `services/mcp/*`, `commands/mcp/*`
- Yode: `crates/yode-mcp/src/*`, `crates/yode-tools/src/builtin/mcp_resources/*`, `crates/yode-tui/src/commands/tools/mcp.rs`

Tasks:

- Add server inventory with transport, auth, scopes, failure reason, last health check.
- Add MCP config reload without restart.
- Add trust prompts for newly discovered servers and resources.
- Add resource/channel permission checks.

Acceptance:

- `/mcp status` shows active, disabled, auth-needed, failed, and unmanaged servers.
- Reload detects added/removed servers and reports diff.
- Resource reads follow explicit allow/deny policy.

Validation:

```bash
cargo test -q -p yode-mcp
cargo test -q -p yode-tools mcp_resources
cargo test -q -p yode-tui mcp
```

Progress:

- 2026-05-19: Added first-class MCP server inventory state for `/mcp status`, including config-level `disabled`, active/configured/unmanaged/auth-needed/failed classification, OAuth scope display, and server-specific reconnect failure handling. Disabled servers are rejected before connection attempts, and status tests cover active, disabled, auth-needed, failed, unmanaged, and OAuth scopes. Verified with `cargo test -q -p yode-mcp`, `cargo test -q -p yode-tools mcp_resources`, `cargo test -q -p yode-tui mcp`, `cargo check -q`, and `git diff --check`. Remaining risk: reload diff and explicit resource allow/deny policy are still pending.
- 2026-05-19: Added explicit MCP resource allow/deny policy via `mcp.resource_allow` and `mcp.resource_deny`, threaded into tool execution context for both TUI and noninteractive chat. `read_mcp_resource` now blocks denied or non-allowlisted resources before cache/provider reads and returns actionable permission guidance. Verified with `cargo test -q -p yode-mcp`, `cargo test -q -p yode-tools mcp_resources`, `cargo test -q -p yode-tui mcp`, `cargo check -q`, and `git diff --check`. Remaining risk: reload diff is still pending.
- 2026-05-19: Added `/mcp reload` diff snapshots under `.yode/status/mcp-reload-state.json`, reporting added, removed, changed, and unchanged MCP servers without restarting the process. The diff captures disabled state, transport, endpoint, auth readiness, and scopes, with focused tests for added/removed/changed detection. Verified with `cargo test -q -p yode-mcp`, `cargo test -q -p yode-tools mcp_resources`, `cargo test -q -p yode-tui mcp`, `cargo check -q`, and `git diff --check`. Remaining risk: reload currently reports and persists diff snapshots; reconnecting/tearing down live MCP clients in-place remains a follow-up.

### Week 10: Plugin System Foundation

Objective: unify plugin, skill, hook, command, and MCP extension discovery.

Reference:

- Claude: `services/plugins/*`, `commands/plugin/*`, `utils/plugins/*`
- Yode: `crates/yode-core/src/skills.rs`, `crates/yode-core/src/hooks/*`, `crates/yode-tui/src/commands/tools/skills.rs`

Tasks:

- Define `.yode/plugins/<name>/plugin.toml` manifest.
- Support plugin-provided skills, hooks, MCP servers, commands, and workflows.
- Add trust state: installed, enabled, disabled, blocked.
- Add `/plugin list`, `/plugin enable`, `/plugin disable`, `/plugin inspect`.

Acceptance:

- A local plugin can provide one skill and one workflow.
- Disabling plugin removes its dynamic contributions.
- Plugin manifest errors appear in `/diagnostics` and `/doctor`.

Validation:

```bash
cargo test -q -p yode-core skills
cargo test -q -p yode-core hooks
cargo test -q -p yode-tui skills
```

Progress:

- 2026-05-19: Added the core plugin manifest discovery layer for `.yode/plugins/<name>/plugin.toml`, including installed/enabled/disabled/blocked trust states, skill/workflow/hook/command/MCP contribution inventory, deterministic ordering, path escape validation, and diagnostics for missing or invalid manifests. Verified with `cargo test -q -p yode-core plugins`, `cargo test -q -p yode-core skills`, and `cargo test -q -p yode-core hooks`. Remaining risk: plugin contributions are inventoried but not yet wired into dynamic skills/workflows, `/plugin` commands, or `/diagnostics`/`/doctor`.
- 2026-05-19: Wired enabled plugin skill and workflow contributions into existing discovery paths. `SkillRegistry::default_paths` now includes enabled plugin `SKILL.md` contributions, workflow discovery loads enabled plugin JSON workflows, and disabled plugins are excluded for both surfaces. Verified with `cargo test -q -p yode-core skills` and `cargo test -q -p yode-tui workflows`. Remaining risk: `/plugin` management commands and diagnostics/doctor surfacing are still pending.
- 2026-05-19: Added `/plugin list|inspect|enable|disable` with trust-state writes routed through the core manifest updater, plus command-registration wiring and focused render tests. Verified with `cargo test -q -p yode-core plugins`, `cargo test -q -p yode-tui plugin`, and `cargo test -q -p yode-tui skills`. Remaining risk: diagnostics/doctor exposure and plugin-provided hooks/MCP/commands are still pending.
- 2026-05-19: Surfaced plugin inventory and manifest diagnostics in `/diagnostics` and `/doctor`, including manifest error counts and first-error previews for broken `.yode/plugins/<name>/plugin.toml` entries. Verified with `cargo test -q -p yode-tui diagnostics`, `cargo test -q -p yode-tui doctor`, `cargo test -q -p yode-core skills`, and `cargo test -q -p yode-core hooks`. Remaining risk: plugin-provided hooks/MCP/commands are still inventoried but not activated dynamically.
- 2026-05-19: Activated plugin-provided hooks by loading enabled plugin hook TOML contributions through `discover_plugin_hooks` and merging them into startup hook registration alongside configured hooks. Disabled plugins are skipped and hook manifest parse/read errors are captured as discovery diagnostics. Verified with `cargo test -q -p yode-core hooks`, `cargo test -q -p yode-core plugins`, and `cargo test -q -p yode-tui skills`. Remaining risk: plugin-provided MCP servers and commands are still inventoried but not activated dynamically.
- 2026-05-19: Activated plugin-provided MCP server manifests by loading enabled plugin `mcp_servers = ["mcp/servers.toml"]` contributions with the existing `[servers.<name>]` schema, merging them into startup MCP connection setup and `/mcp` status without overriding project/user config. Verified with `cargo test -q -p yode-core plugins`, `cargo test -q -p yode-core hooks`, `cargo test -q -p yode-tui skills`, and `cargo test -q -p yode-tui mcp`. Remaining risk: plugin-provided slash commands are still inventoried but not dynamically registered.
- 2026-05-19: Activated plugin-provided slash commands by loading enabled plugin `commands = ["commands/*.toml"]` contributions with `[[commands]] name/description/message|prompt`, adding them to command completion, and registering prompt/message command wrappers without overriding existing built-in or dynamic commands. Verified with `cargo test -q -p yode-core plugins`, `cargo test -q -p yode-tui plugin`, `cargo test -q -p yode-tui skills`, `cargo test -q -p yode-tui mcp`, and `cargo check -q`. Remaining risk: plugin commands are prompt/message commands only; richer command handlers remain a future extension.

### Week 11: Skills Search And Invocation Persistence

Objective: make skills behave like first-class dynamic capabilities.

Reference:

- Claude: `services/skillSearch/*`, `tools/SkillTool/*`, `tools/DiscoverSkillsTool/*`
- Yode: `crates/yode-core/src/skills.rs`, `crates/yode-tools/src/builtin/skill/*`, `crates/yode-tui/src/commands/tools/skills.rs`

Tasks:

- Track skill invocation per session and per subagent.
- Persist invoked skill content or head excerpts through compaction budget.
- Add skill search by name, description, path patterns, and trigger examples.
- Add stale skill diagnostics when referenced files disappear.

Acceptance:

- Invoked skills survive compact with budgeted truncation.
- `/skills active` explains why each skill is active.
- `discover_skills` returns deterministic ranked results.

Validation:

```bash
cargo test -q -p yode-core skills
cargo test -q -p yode-tools skill
cargo test -q -p yode-tui skills
```

Progress:

- 2026-05-19: Added deterministic skill search over names, descriptions, path gates, and `trigger-examples`/trigger aliases in skill frontmatter, with scored reasons and `/skills search <query>` output. Verified with `cargo test -q -p yode-core skills`, `cargo test -q -p yode-tools skill`, and `cargo test -q -p yode-tui skills`. Remaining risk: `discover_skills` still lists store order, and skill invocation persistence across compaction is still pending.

### Week 12: Remote Event Log And Replay

Objective: make remote-control and task flows resumable from durable event logs.

Reference:

- Claude: `bridge/*`, `remote/*`, `QueryEngine.ts`, `utils/sessionStoragePortable.ts`
- Yode: `crates/yode-tools/src/builtin/remote_runtime/*`, `crates/yode-tui/src/commands/dev/remote_control*`, `crates/yode-tools/src/runtime_tasks/*`

Tasks:

- Define event-log schema for remote task lifecycle, status, output chunks, reconnects, compactions, and terminal signals.
- Serialize event logs as JSONL under `.yode/remote` or `.yode/tasks`.
- Add replay command and test fixtures.
- Add remote storage backend abstraction.

Acceptance:

- A remote task can be reconstructed from event log after process restart.
- Replay detects missing events and emits actionable diagnostics.
- Remote monitor displays reconnect count and last event cursor.

Validation:

```bash
cargo test -q -p yode-tools remote_runtime
cargo test -q -p yode-tui remote_control
bash scripts/parity-replay-ci.sh
```

Month 3 milestone:

- Extension points have a trust lifecycle.
- Skills persist across compact.
- Remote/task state is replayable and resumable.

## Month 4: Product Polish, CI Hardening, And Release Readiness

### Week 13: Missing Product Commands With Yode-Native Scope

Objective: add the highest-value Claude command equivalents without copying account-specific features.

Reference:

- Claude commands: `plugin`, `branch`, `add-dir`, `files`, `tag`, `stats`, `usage`, `keybindings`, `ide`
- Yode command registry: `crates/yode-tui/src/commands/mod.rs`

Tasks:

- Add `/files`: list files currently in context and restore source.
- Add `/tag`: add searchable session tags in SQLite.
- Add `/branch`: branch current session/checkpoint, using existing checkpoint infrastructure.
- Add `/keybindings`: inspect local keymap and config path.
- Add `/stats`: aggregate local usage, task, tool, and compact stats.

Acceptance:

- Commands have help text, completion metadata, and focused tests.
- Commands degrade gracefully in non-git or empty-session workspaces.
- `/help` and `/status` cross-link the new commands where relevant.

Validation:

```bash
cargo test -q -p yode-tui commands
cargo test -q -p yode-core db
```

### Week 14: Interactive Diagnostics And Context Visualization

Objective: make existing observability faster to consume.

Reference:

- Claude: `components/ContextVisualization.tsx`, `components/DiagnosticsDisplay.tsx`, `commands/context/*`
- Yode: `crates/yode-tui/src/commands/info/context_cmd.rs`, `crates/yode-tui/src/commands/info/diagnostics_render.rs`, `crates/yode-tui/src/ui/status_summary.rs`

Tasks:

- Add compact context bar with system/user/assistant/tool/restore proportions.
- Add diagnostics severity grouping and quick action hints.
- Add inspector navigation from each diagnostic row to artifact/file.
- Add terminal-width tests for CJK and narrow viewports.

Acceptance:

- `/context` gives both textual and visual density without wrapping badly.
- `/diagnostics` top 5 issues are actionable.
- Snapshot tests cover narrow, normal, and wide terminal widths.

Validation:

```bash
cargo test -q -p yode-tui context_cmd
cargo test -q -p yode-tui diagnostics
bash scripts/parity-visual-ci.sh
```

### Week 15: CI Contract Consolidation

Objective: reduce parity script sprawl into clear, enforceable contracts.

Reference:

- Current scripts: `scripts/parity-*`
- CI: `.github/workflows/ci.yml`

Tasks:

- Group parity checks into contract categories: command output, replay, visual, artifacts, docs.
- Add one manifest file mapping each contract to owner, fixtures, scripts, and CI job.
- Remove or deprecate redundant scripts only after compatibility wrappers exist.
- Add failure triage templates generated from CI artifacts.

Acceptance:

- New AI contributor can run one command to see all parity contract failures.
- CI uploads enough artifacts to diagnose without rerunning locally.
- Risk register links to contract ids.

Validation:

```bash
bash scripts/parity-ci-local.sh
bash scripts/parity-docs-ci.sh
bash scripts/parity-risk-register-validate.sh
```

### Week 16: Release Candidate And Hardening Sprint

Objective: produce a release-ready parity milestone.

Tasks:

- Run full workspace tests on macOS, Linux, and Windows where available.
- Audit config migrations and backward compatibility.
- Refresh README and release notes with only user-visible changes.
- Run long-session benchmark before and after the 4-month work.
- Create a final gap report with accepted non-goals.

Acceptance:

- `cargo test --workspace --lib` passes.
- CI parity jobs are green.
- Long-session benchmark shows improved context survival or stable behavior with richer diagnostics.
- Release notes explain changes without overclaiming Claude compatibility.

Validation:

```bash
cargo test --workspace --lib
cargo clippy -p yode -p yode-core -p yode-llm -p yode-tools -p yode-tui -p yode-mcp -p yode-agent --no-deps -- -D warnings
bash scripts/parity-ci-local.sh
bash scripts/release-checklist.sh
```

Month 4 milestone:

- Product commands cover the most useful Claude equivalents.
- Diagnostics are actionable.
- Parity contracts are maintainable.
- Release candidate is documented and benchmarked.

## AI Execution Protocol

Every AI agent following this report should use this loop:

1. Read the week objective and only the listed source files.
2. Produce a two-to-five item implementation plan.
3. Make one narrow change.
4. Run the targeted tests listed for that task.
5. Run `cargo check -q` and `git diff --check`.
6. Commit with a short imperative message.
7. Update or add a closeout note only if the task changed product behavior or contract coverage.

Commit rules:

- Keep each commit to one behavior or one test/fixture update.
- Never commit `.serena/project.yml`.
- Never combine refactors with behavior changes.
- If a change touches `EngineRuntimeState`, update every test fixture initializer in the same commit.
- If a user-facing command changes output, add or update a parity snapshot or focused render test.

Preferred commit messages:

- `Add compact boundary records`
- `Budget post-compact restore blocks`
- `Preserve plan state through compact`
- `Classify shell command semantics`
- `Add permission rule editor`
- `Support forked subagent context`
- `Persist remote event logs`
- `Add files context command`
- `Consolidate parity contracts`

## Definition Of Done For Each Workstream

Context:

- typed compact/collapse records
- shared restore budget
- plan/task/skill restore
- benchmark evidence

Tools and permissions:

- semantic shell classifier
- explainable rule decisions
- focused tests for destructive, read-only, and unknown commands
- user-facing remediation text

Subagents:

- forked context mode
- recursion guard
- worktree notice
- task output and cancellation preserved

MCP/plugins/skills:

- inventory and trust state
- reload path
- compaction persistence
- diagnostics and doctor entries

Remote/tasks:

- durable JSONL event log
- replay command
- reconnect state
- artifact upload bundle

CI:

- contract manifest
- local one-command parity run
- uploaded failure artifacts
- risk register linkage

## Risk Register

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Context features bloat prompts | Compact can retrigger immediately | Enforce shared restore budget and signed delta telemetry |
| Forked subagents duplicate work or recurse | Wasted tokens and unsafe edits | Add fork boilerplate guard and task ownership prompts |
| Permission classifier over-allows shell commands | Safety regression | Default unknown to ask/deny; add negative tests before allow rules |
| Plugin system expands trust surface | Security and config drift | Require explicit enable/trust and source diagnostics |
| Remote event logs become incompatible | Resume/replay failure | Version event schema and add migration tests |
| CI scripts remain too fragmented | AI contributors cannot know what failed | Build contract manifest and one local runner |
| Product commands copy Claude account-only behavior | Scope creep | Keep Yode-native equivalents and document non-goals |

## Non-Goals

Do not spend the four months on these unless explicitly requested:

- cloning Claude account login, billing, Max upgrade, mobile, stickers, voice, or Anthropic-only product flows
- replacing Rust TUI with a React/Ink UI
- adding telemetry that sends user data externally
- broad rewrites of the engine module layout without behavior gains
- removing existing parity scripts before a compatibility wrapper exists

## First Two Weeks Detailed Starting Queue

This queue is intended for the next AI agent to start immediately.

### Task A: Compact Boundary Record

Files:

- `crates/yode-core/src/engine/types.rs`
- `crates/yode-core/src/engine/compaction_runtime.rs`
- `crates/yode-core/src/transcript/mod.rs`
- `crates/yode-core/src/transcript/render.rs`
- `crates/yode-core/src/engine/tests/compaction.rs`

Implementation sketch:

- Add `CompactBoundaryRuntimeState` or a transcript record struct.
- Build the record in `finalize_compaction_result`.
- Include it in transcript rendering and restore artifacts.
- Add tests asserting mode, removed count, post-compact token delta, and artifact path.

Commit target:

- `Add compact boundary records`

Progress:

- 2026-05-16: Added compact boundary runtime/session records with transcript and restore artifact rendering. Verified with `cargo test -q -p yode-core compaction`, `cargo test -q -p yode-core transcript`, `cargo test -q -p yode-core db`, `cargo test -q -p yode-tui memory`, `cargo check -q`, and `git diff --check`. Remaining risk: persisted DB boundary JSON is latest-boundary metadata only; full replay still relies on transcript artifacts until a richer event log lands.

### Task B: Restore Budget Planner

Files:

- `crates/yode-core/src/engine/compaction_runtime.rs`
- `crates/yode-core/src/engine/tests/compaction.rs`
- `crates/yode-tui/src/commands/info/context_cmd.rs`

Implementation sketch:

- Introduce `RestoreBudget { total, used, entries }`.
- Make each restore block request budget.
- Add truncation markers with recovery instructions.
- Surface budget in `/context`.

Commit target:

- `Budget post-compact restore blocks`

Progress:

- 2026-05-16: Added a shared post-compact restore budget with per-block caps, truncation recovery hints, restore artifact budget table, state artifact budget JSON, and `/context` budget summary. Verified with `cargo test -q -p yode-core compaction`, `cargo test -q -p yode-tui context_cmd`, `bash scripts/verify-compact-artifacts.sh`, `cargo check -q`, and `git diff --check`. Remaining risk: token accounting is approximate character-based budgeting; future model-specific tokenizers can tighten the cap.

### Task C: Plan Restore Block

Files:

- `crates/yode-tools/src/builtin/plan_mode/*`
- `crates/yode-core/src/engine/compaction_runtime.rs`
- `crates/yode-core/src/engine/runtime_support.rs`
- `crates/yode-tui/src/commands/session/plan.rs`

Implementation sketch:

- Capture plan mode and plan file path in runtime state.
- Add restore block after compact.
- Add `/plan status` details for compacted sessions.

Commit target:

- `Preserve plan state through compact`

Progress:

- 2026-05-16: Added a plan runtime snapshot used by post-compact restore blocks and `/plan status`, including plan mode, permission mode, active plan file discovery, and compact-restore availability. Verified with `cargo test -q -p yode-core compaction`, `cargo test -q -p yode-tools plan_mode`, `cargo test -q -p yode-tui plan`, `cargo check -q`, and `git diff --check`. Remaining risk: active plan file discovery is read-only and limited to existing Yode/common plan paths; this does not introduce a dedicated plan-file writer.

### Task D: Async Task Restore Block

Files:

- `crates/yode-tools/src/runtime_tasks/*`
- `crates/yode-core/src/engine/compaction_runtime.rs`
- `crates/yode-tui/src/commands/info/tasks.rs`
- `crates/yode-tui/src/commands/info/tasks_render.rs`

Implementation sketch:

- Snapshot running/completed-unretrieved tasks during compact.
- Restore task ids, output paths, progress, and retrieval status.
- Add tests for active subagent compact.

Commit target:

- `Restore async task state after compact`

Progress:

- 2026-05-16: Added a post-compact async task restore block with task id, status, output path, transcript path, last progress, retrieval guidance, and duplicate-spawn guard text. `/context` and `/tasks summary/follow latest` now surface compact task restore hints. Verified with `cargo test -q -p yode-core compaction`, `cargo test -q -p yode-tools runtime_tasks`, `cargo test -q -p yode-tui tasks`, `cargo check -q`, and `git diff --check`. Remaining risk: task retrieval state is expressed as operator guidance rather than a persisted per-task read/unread flag.

## Final Recommendation

Start with Month 1 before adding more product commands. Yode's current differentiator is local-first Rust runtime plus operator visibility. The fastest path to "feels as capable as Claude Code" is not a wider slash-command list; it is making long, messy coding sessions survive context pressure, background agents, remote work, and recovery without losing state.
