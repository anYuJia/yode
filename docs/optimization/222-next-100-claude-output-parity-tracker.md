# Next 100 Claude Output / Interaction Parity Tracker

## Scope

这份文档从当前 `yode` 基线继续往 `claude-code-rev` 的终端输出与交互体验对齐。

只跟踪这些方向：

- markdown / table / hyperlink / reasoning 渲染
- tool / system / error / inspector / turn-status 呈现
- `ctrl+o` 可发现性与最近消息展开体验
- confirm / runtime / artifact 文案压缩与一致性

不把后端能力本身当作本清单的目标，除非它直接影响输出层体验。

状态标记：

- `[ ]` 未开始
- `[~]` 进行中
- `[x]` 已完成

当前进度：

- `36 / 100` 已完成

## 001-010 Markdown Core

- `[x]` 001 markdown block cache
- `[x]` 002 plain-text fast path
- `[x]` 003 blockquote italic style
- `[x]` 004 stronger H1 emphasis
- `[x]` 005 mailto link plain-text fallback
- `[x]` 006 GitHub issue reference hyperlinking
- `[x]` 007 bare URL hyperlinking
- `[x]` 008 trailing punctuation URL guard
- `[x]` 009 boxed table borders
- `[x]` 010 narrow table vertical fallback

## 011-020 Tool Output Surfaces

- `[x]` 011 grouped tool summary hints
- `[x]` 012 grouped tool live progress lines
- `[x]` 013 single tool `ctrl+o` discoverability
- `[x]` 014 grouped tool `ctrl+o` discoverability
- `[x]` 015 scrollback grouped tool parity
- `[x]` 016 turn-status grouped tool hint line
- `[x]` 017 turn-status prefers live tool progress
- `[x]` 018 turn-status compact tool labels
- `[x]` 019 inspector tool output markdown rendering
- `[x]` 020 inspector tool metadata badges

## 021-030 Thinking / Assistant

- `[x]` 021 assistant thinking markdown rendering
- `[x]` 022 thinking uses dim markdown path
- `[x]` 023 thinking `ctrl+o` discoverability
- `[x]` 024 assistant reasoning inspector panel
- `[x]` 025 assistant content inspector panel
- `[x]` 026 latest assistant detail fallback without reasoning
- `[ ]` 027 transcript-mode thinking collapse policy
- `[ ]` 028 streaming reasoning teaser line
- `[ ]` 029 reasoning summary chip in inspector
- `[ ]` 030 assistant detail badges

## 031-040 Error Presentation

- `[x]` 031 context-limit error specialization
- `[x]` 032 auth error specialization
- `[x]` 033 rate-limit error specialization
- `[x]` 034 quota / billing error specialization
- `[x]` 035 timeout error specialization
- `[x]` 036 error `ctrl+o` discoverability
- `[x]` 037 error detail inspector
- `[ ]` 038 error recovery action hints by class
- `[ ]` 039 compact retry-state phrasing parity
- `[ ]` 040 API error truncation / expansion parity

## 041-050 System Messages

- `[x]` 041 compact long paths in system details
- `[x]` 042 compact runtime artifact paths
- `[x]` 043 grouped system `ctrl+o` discoverability
- `[x]` 044 single system `ctrl+o` discoverability
- `[x]` 045 system detail inspector
- `[ ]` 046 system message raw/detail split refinement
- `[ ]` 047 stop-hook summary panel polish
- `[ ]` 048 export message panel polish
- `[ ]` 049 turn-complete system card polish
- `[ ]` 050 lifecycle message simplification pass

## 051-060 Confirm / Approval UX

- `[x]` 051 compact confirm primary file paths
- `[x]` 052 compact confirm preview file paths
- `[x]` 053 compact inspector follow-up file paths
- `[x]` 054 pending confirmation inspector
- `[ ]` 055 confirm preview for LSP path compaction
- `[ ]` 056 confirm preview for URL emphasis
- `[ ]` 057 confirm risk hierarchy polish
- `[ ]` 058 confirm action wording parity
- `[ ]` 059 confirm panel dense/narrow layout pass
- `[ ]` 060 confirm shell command folding parity

## 061-070 Inspector Runtime

- `[x]` 061 latest assistant details inspector
- `[x]` 062 latest system details inspector
- `[x]` 063 latest error details inspector
- `[x]` 064 latest tool activity inspector enriches active progress
- `[x]` 065 latest-message inspector entrypoint semantics
- `[x]` 066 inspector panel markdown rendering helper
- `[x]` 067 inspector badges for state / severity
- `[ ]` 068 inspector footer command density pass
- `[ ]` 069 inspector action copy parity pass
- `[ ]` 070 inspector tab naming consistency pass

## 071-080 Scrollback Parity

- `[x]` 071 scrollback assistant reasoning markdown
- `[x]` 072 scrollback tool grouped hint parity
- `[x]` 073 scrollback tool grouped progress parity
- `[x]` 074 scrollback system path compaction parity
- `[x]` 075 scrollback error inspect affordance
- `[ ]` 076 scrollback assistant detail affordance parity
- `[ ]` 077 scrollback tool metadata density pass
- `[ ]` 078 scrollback system batch density pass
- `[ ]` 079 scrollback error framing parity
- `[ ]` 080 scrollback narrow-width polish

## 081-090 Artifact / Export Output

- `[x]` 081 bundle exports live under `.yode/exports`
- `[x]` 082 conversation exports live under `.yode/exports`
- `[x]` 083 doctor bundle exports live under `.yode/exports`
- `[x]` 084 bundle lookup resolves from session root
- `[x]` 085 bundle lookup resolves from nested cwd
- `[ ]` 086 export workspace index density pass
- `[ ]` 087 doctor bundle summary density pass
- `[ ]` 088 remote bundle summary density pass
- `[ ]` 089 artifact preview truncation parity
- `[ ]` 090 exported transcript readability pass

## 091-100 Final Polish

- `[ ]` 091 status-line wording parity audit
- `[ ]` 092 spinner verb audit against claude-code-rev
- `[ ]` 093 tool label capitalization audit
- `[ ]` 094 path compaction helper unification
- `[ ]` 095 hyperlink coverage audit
- `[ ]` 096 system/error/tool inspector discoverability audit
- `[ ]` 097 narrow terminal snapshot pass
- `[ ]` 098 output regression screenshot/script pass
- `[ ]` 099 final parity gap review
- `[ ]` 100 final closeout + release-note draft

## Notes

- 已完成项主要对应最近这一串输出层提交：
  - `113364c`, `19e07ac`, `e5b5c19`, `e52bafa`
  - `08528ac`, `5a02f3d`, `5056807`
  - `88d5df4`, `8145b11`
  - `9c8cd23`, `29a2dc4`
  - `2ef37f8`, `2b78689`
  - `61a8fed`, `066626b`, `183819e`
  - `5c9a0df`, `513540a`, `463250e`, `10fa05b`
  - `775d61e`, `3970110`

- 下一步优先实现：
  - `068 inspector footer command density pass`
