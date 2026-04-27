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

- `74 / 100` 已完成

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

- `[x]` 021 hook failure system wording pass
- `[x]` 022 hook deferred system wording pass
- `[x]` 023 hook inspector action density pass
- `[x]` 024 background-task brief cadence polish
- `[x]` 025 task runtime summary compactness pass
- `[x]` 026 task retry wording compactness pass
- `[x]` 027 task artifact backlink density pass
- `[x]` 028 task output preview truncation parity
- `[x]` 029 task transcript preview truncation parity
- `[x]` 030 remote handoff summary wording parity

## 031-040 Remote / Workflow Output

- `[x]` 031 remote transport status wording pass
- `[x]` 032 remote queue summary density pass
- `[x]` 033 remote retry summary density pass
- `[x]` 034 remote bundle inspector affordance pass
- `[x]` 035 workflow preview output density pass
- `[x]` 036 workflow inspector action copy polish
- `[x]` 037 coordinator summary density pass
- `[x]` 038 orchestration timeline wording pass
- `[x]` 039 review artifact preview density pass
- `[x]` 040 review result framing parity

## 041-050 Inspector Phase Two

- `[x]` 041 inspector focus line wording audit
- `[x]` 042 inspector search status density pass
- `[x]` 043 inspector action keyboard hint parity
- `[x]` 044 inspector selected-line framing polish
- `[x]` 045 inspector badge ordering audit
- `[x]` 046 inspector footer stale-action warning
- `[x]` 047 inspector raw panel title consistency
- `[x]` 048 inspector panel count density pass
- `[x]` 049 inspector empty-state copy polish
- `[ ]` 050 inspector latest-open stack wording pass

## 051-060 Confirm Phase Two

- `[x]` 051 confirm shell risk copy refinement
- `[x]` 052 confirm network risk copy refinement
- `[x]` 053 confirm file-write risk copy refinement
- `[x]` 054 confirm option emphasis hierarchy polish
- `[x]` 055 confirm inline command wrapping parity
- `[x]` 056 confirm inspector reopen affordance
- `[x]` 057 confirm host/path mixed preview density pass
- `[x]` 058 confirm background-task approval copy pass
- `[x]` 059 confirm multiline preview continuation style
- `[x]` 060 confirm final safety copy audit

## 061-070 Artifact / Export Phase Two

- `[x]` 061 workspace index jump target ordering audit
- `[x]` 062 conversation export role heading polish
- `[x]` 063 conversation export spacing polish
- `[x]` 064 diagnostics bundle completion copy audit
- `[x]` 065 doctor handoff template density pass
- `[x]` 066 artifact history line ordering audit
- `[x]` 067 artifact preview label consistency
- `[x]` 068 artifact freshness badge wording pass
- `[x]` 069 export filename/path compactness audit
- `[x]` 070 benchmark snapshot output polish

## 071-080 Hyperlinks / Markdown Edge Cases

- `[x]` 071 system detail hyperlink coverage expansion
- `[x]` 072 error detail hyperlink coverage expansion
- `[x]` 073 confirm preview hyperlink coverage audit
- `[x]` 074 artifact preview hyperlink coverage audit
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
- `[x]` 084 assistant teaser copy audit
- `[x]` 085 tool progress wording audit
- `[x]` 086 export/bundle noun audit
- `[x]` 087 retry wording audit across task/remote/runtime
- `[x]` 088 hook wording audit across system/inspector/workspace
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
