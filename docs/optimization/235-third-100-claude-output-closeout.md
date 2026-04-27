# Third 100 Claude Output Closeout

## Gap Review

Third-round parity work is complete across transcript semantics, markdown typography, remote/workflow/review surfaces, inspector/confirm copy, export/artifact narratives, hooks/tasks/recovery wording, snapshot infrastructure, wording audits, and regression coverage.

Residual risks:

- Visual parity is now primarily snapshot-driven; terminal palette differences can still shift perceived emphasis.
- Markdown nesting and table behavior are covered by focused unit tests, but very large mixed CJK/ASCII tables should remain part of manual spot checks.
- Closeout coverage validates operator-facing strings, not full end-to-end remote infrastructure behavior.

## Release Note Draft

Yode TUI now has the third 100-pass Claude-output parity polish set:

- Transcript output keeps latest-focus detail for tool/system/error runs while compacting older noise.
- Markdown rendering covers heading wraps, nested bullets, CJK tables, inline code, blockquotes, code fence captions, links, and paragraph spacing.
- Remote, workflow, review, export, artifact, hooks, tasks, recovery, inspector, and confirmation surfaces use denser operator-facing language with regression coverage.
- Snapshot catalog scripts and generated docs now provide sectioned parity artifacts for ongoing review.

## Maintenance Checklist

- Run `cargo test -p yode-tui --quiet` before closing future parity batches.
- Re-run focused snapshot catalog scripts after changing transcript, export, remote, or inspector rendering.
- Keep tracker counts aligned with checked rows before committing closeout changes.
- Prefer adding focused regression tests beside the renderer/helper being polished.

## Transcript Checklist

- Latest assistant/tool/system/error detail remains visible.
- Older reasoning/system/error detail collapses to an inspectable teaser.
- Ask-user, subagent, progress, and compaction boundary lines keep stable prefixes.
- Batch titles identify remote, review, workflow, hook, task, and status groups.

## Markdown Checklist

- Heading continuation lines preserve heading style.
- Nested lists keep distinct markers and indentation.
- Inline code and mixed bold/italic survive wrapping.
- CJK tables, narrow tables, blockquotes, links, and code fences keep dense readable output.

## Remote / Workflow / Review Checklist

- Remote state and queue summaries stay compact.
- Workflow show/preview/write-mode output stays action-oriented.
- Review artifacts preserve fold markers, badges, and residual-risk wording.
- Runtime orchestration artifacts retain clear jump targets.

## Inspector / Confirm Checklist

- Inspector footer uses panel-oriented navigation wording.
- Raw markdown panels are labeled as raw views.
- Confirm previews compact shell, delegated, path, URL, and mixed target details.
- Narrow confirm density and option hierarchy remain covered by focused tests.

## Closeout Review

The third 100 is ready to hand off. Remaining work should move to a fourth tracker focused on live snapshot drift, E2E transcript replay, and higher-fidelity visual parity checks rather than additional one-off wording passes.
