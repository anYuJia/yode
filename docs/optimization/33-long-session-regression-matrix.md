# Long-Session Regression Matrix

## Matrix

| Area | Scenario | Expected |
| --- | --- | --- |
| Compact | auto compact with transcript write | session memory + transcript artifact present |
| Compact | repeated no-op compact | breaker opens after threshold |
| Memory | live refresh after large turn | `session.live.md` updated |
| Memory | oversized memory content | newest entry preserved, old entries truncated |
| Tools | large tool result | truncation metadata recorded |
| Tools | parallel read-only tools | parallel telemetry increments |
| Permissions | repeated denial | auto-skip and deny history visible |
| Recovery | repeated validation failures | single-step state visible in `/status` |
| Tasks | background bash | `/tasks` shows running/completed task |
| Tasks | background agent | task output persisted to `./.yode/tasks` |
| Compare | huge transcript compare | diff preview skipped with size-cap note |
| Hooks | compact hook runtime metadata | recovery/tool/memory runtime fields present |

## Suggested automation

- `scripts/verify-compact-artifacts.sh`
- targeted `cargo test` on engine + yode-tools
- smoke run on a temp repo with synthetic transcript artifacts
