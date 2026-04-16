# Round 9 Tool UX Closeout

## Scope

这份文档对应 round-9 tracker 的 `071-080`，记录 tool UX 与 operator surface 这一批的收口状态。

## Closed

- 状态栏与 turn status 统一使用 `Default / Auto / Plan`、`jobs`、`team/live/defer` 术语
- runtime jump inventory 统一覆盖 team、remote live、hook defer、permission governance、tool-search activation
- `/tasks monitor` 与 `/remote-control monitor` 补成显式 monitor/follow surface
- remote queue 的 operator-facing 状态统一为 `queued / dispatched / running / completed / failed / acknowledged / needs-attention`
- `/permissions mode` 从单行帮助升级为 operator guide
- hidden/deferred tool affordance 进入 `/tools diag`

## Verification

- `cargo test -p yode-tui --lib`
- `cargo check -p yode-tui -p yode`

## Residual Gaps

- 仍然主要是 slash command / inspector 驱动，不是 IDE-native command palette / notification center
- background work 仍缺 richer push notification 与多面板 watch surface
