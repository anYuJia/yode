# Round 9 First-Class Tool Verification Matrix

## Unit Coverage

- remote queue operator labels: `commands::dev::remote_control_workspace::tests::remote_queue_status_labels_are_operator_friendly`
- permission mode guide surface: `commands::tools::permissions::tests::permission_mode_guide_mentions_keyboard_and_slash_only_modes`
- hidden/deferred tool affordance: `commands::tools::tools::tests::tool_search_affordance_mentions_hidden_and_deferred_surface`
- runtime jump inventory: `commands::workspace_nav::tests::task_review_and_transcript_targets_include_expected_commands`

## Runtime / Artifact Coverage

- remote live session continuity and transcript sync
- agent team monitor artifacts
- hook defer state / inspect aliases
- permission governance artifacts
- tool-search activation artifacts

## Verification Commands

- `cargo test -p yode-core --lib`
- `cargo test -p yode-tools --lib`
- `cargo test -p yode-tui --lib`
- `cargo test -p yode --bin yode`

## Latest Sweep

- 已于 `2026-04-16` 重新跑完以上矩阵。
