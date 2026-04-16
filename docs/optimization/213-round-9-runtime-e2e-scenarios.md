# Round 9 Runtime E2E Scenarios

## Scenario 1: Remote Live Session

1. `/remote-control plan <goal>`
2. `/remote-control transport connect`
3. `/remote-control dispatch latest`
4. `/remote-control follow latest`
5. `/remote-control complete latest <summary>`
6. `/inspect artifact latest-remote-live-session-state`

## Scenario 2: Permission Governance

1. `/permissions mode auto`
2. `/permissions explain bash`
3. `/permissions governance`
4. `/inspect artifact latest-permission-governance`
5. `/status`

## Scenario 3: Hook Defer Recovery

1. 触发会进入 defer 的 tool flow
2. `/inspect artifact latest-hook-deferred`
3. `/inspect artifact latest-hook-deferred-state`
4. `/diagnostics`
5. `/brief`

## Scenario 4: Team Runtime

1. 通过 `team_create` / `send_message` / `team_monitor` 建立 team runtime
2. `/inspect artifact latest-agent-team-monitor`
3. `/tasks monitor`
4. `/inspect artifact latest-subagent-result`
