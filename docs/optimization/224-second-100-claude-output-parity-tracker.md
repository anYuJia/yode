# Second 100 Claude Output / Interaction Parity Tracker

## Scope

这第二个 100 轮不再重复第一轮已经完成的基础输出对齐，而是继续往更深层的 `claude-code-rev` 体验逼近：

- transcript / scrollback / working-state 语义微调
- snapshot / regression / golden output 基建
- hook / remote / task / workflow / review 输出密度
- inspector / confirm / artifact / export 第二阶段 polish
- wording / localization / consistency 审核

状态标记：

- `[ ]` 未开始
- `[~]` 进行中
- `[x]` 已完成

当前进度：

- `15 / 100` 已完成

## 001-010 Snapshot Hardening

- `[x]` 001 output regression snapshot harness filtering
- `[x]` 002 turn-status snapshot section in regression script
- `[x]` 003 inspector snapshot coverage for assistant/tool/system/error
- `[x]` 004 confirm panel snapshot coverage for shell/network/write cases
- `[x]` 005 grouped tool snapshot coverage at narrow widths
- `[x]` 006 grouped system snapshot coverage at narrow widths
- `[x]` 007 export summary snapshot coverage
- `[x]` 008 remote bundle snapshot coverage
- `[x]` 009 hook summary snapshot coverage
- `[x]` 010 transcript collapse snapshot coverage

## 011-020 Transcript Semantics

- `[x]` 011 transcript-mode tool teaser collapse parity
- `[x]` 012 transcript-mode system detail elision parity
- `[x]` 013 transcript-mode error detail collapse parity
- `[x]` 014 transcript-mode assistant spacing polish
- `[ ]` 015 transcript-mode latest-tool focus parity
- `[ ]` 016 transcript-mode latest-error focus parity
- `[ ]` 017 transcript-mode latest-system focus parity
- `[ ]` 018 transcript-mode follow-up prompt density pass
- `[x]` 019 transcript-mode reasoning teaser copy pass
- `[ ]` 020 transcript-mode line-prefix audit

## 021-030 Hook / Task Output

- `[ ]` 021 hook failure system wording pass
- `[ ]` 022 hook deferred system wording pass
- `[ ]` 023 hook inspector action density pass
- `[ ]` 024 background-task brief cadence polish
- `[ ]` 025 task runtime summary compactness pass
- `[ ]` 026 task retry wording compactness pass
- `[ ]` 027 task artifact backlink density pass
- `[ ]` 028 task output preview truncation parity
- `[ ]` 029 task transcript preview truncation parity
- `[ ]` 030 remote handoff summary wording parity

## 031-040 Remote / Workflow Output

- `[ ]` 031 remote transport status wording pass
- `[ ]` 032 remote queue summary density pass
- `[ ]` 033 remote retry summary density pass
- `[ ]` 034 remote bundle inspector affordance pass
- `[ ]` 035 workflow preview output density pass
- `[ ]` 036 workflow inspector action copy polish
- `[ ]` 037 coordinator summary density pass
- `[ ]` 038 orchestration timeline wording pass
- `[ ]` 039 review artifact preview density pass
- `[ ]` 040 review result framing parity

## 041-050 Inspector Phase Two

- `[ ]` 041 inspector focus line wording audit
- `[ ]` 042 inspector search status density pass
- `[ ]` 043 inspector action keyboard hint parity
- `[ ]` 044 inspector selected-line framing polish
- `[ ]` 045 inspector badge ordering audit
- `[ ]` 046 inspector footer stale-action warning
- `[ ]` 047 inspector raw panel title consistency
- `[ ]` 048 inspector panel count density pass
- `[ ]` 049 inspector empty-state copy polish
- `[ ]` 050 inspector latest-open stack wording pass

## 051-060 Confirm Phase Two

- `[ ]` 051 confirm shell risk copy refinement
- `[ ]` 052 confirm network risk copy refinement
- `[ ]` 053 confirm file-write risk copy refinement
- `[ ]` 054 confirm option emphasis hierarchy polish
- `[ ]` 055 confirm inline command wrapping parity
- `[ ]` 056 confirm inspector reopen affordance
- `[ ]` 057 confirm host/path mixed preview density pass
- `[ ]` 058 confirm background-task approval copy pass
- `[ ]` 059 confirm multiline preview continuation style
- `[ ]` 060 confirm final safety copy audit

## 061-070 Artifact / Export Phase Two

- `[ ]` 061 workspace index jump target ordering audit
- `[ ]` 062 conversation export role heading polish
- `[ ]` 063 conversation export spacing polish
- `[ ]` 064 diagnostics bundle completion copy audit
- `[ ]` 065 doctor handoff template density pass
- `[ ]` 066 artifact history line ordering audit
- `[ ]` 067 artifact preview label consistency
- `[ ]` 068 artifact freshness badge wording pass
- `[ ]` 069 export filename/path compactness audit
- `[ ]` 070 benchmark snapshot output polish

## 071-080 Hyperlinks / Markdown Edge Cases

- `[ ]` 071 system detail hyperlink coverage expansion
- `[ ]` 072 error detail hyperlink coverage expansion
- `[ ]` 073 confirm preview hyperlink coverage audit
- `[ ]` 074 artifact preview hyperlink coverage audit
- `[ ]` 075 transcript export hyperlink preservation audit
- `[ ]` 076 markdown bullet spacing parity pass
- `[ ]` 077 markdown heading wrap polish
- `[ ]` 078 markdown inline-code wrap polish
- `[ ]` 079 markdown mixed CJK table spacing pass
- `[ ]` 080 markdown issue-link punctuation edge cases

## 081-090 Wording / Consistency

- `[ ]` 081 status/help copy casing audit
- `[ ]` 082 system title casing audit
- `[ ]` 083 error title casing audit
- `[ ]` 084 assistant teaser copy audit
- `[ ]` 085 tool progress wording audit
- `[ ]` 086 export/bundle noun audit
- `[ ]` 087 retry wording audit across task/remote/runtime
- `[ ]` 088 hook wording audit across system/inspector/workspace
- `[ ]` 089 compact token wording audit
- `[ ]` 090 punctuation / ellipsis consistency audit

## 091-100 Closeout

- `[ ]` 091 second-round parity gap review
- `[ ]` 092 second-round release note draft
- `[ ]` 093 third-round candidate backlog seed
- `[ ]` 094 regression script usage note
- `[ ]` 095 narrow snapshot catalog
- `[ ]` 096 inspector snapshot catalog
- `[ ]` 097 export snapshot catalog
- `[ ]` 098 remote snapshot catalog
- `[ ]` 099 second-round closeout review
- `[ ]` 100 handoff into third 100 tracker
