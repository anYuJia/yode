# Fourth 100 Claude Output / Interaction Parity Tracker

## Scope

Fourth-round work should move from wording completion into drift prevention and live replay:

- snapshot replay against representative transcripts
- terminal visual regression sampling
- end-to-end remote/workflow/review/operator flows
- parity guardrails for future renderer changes

当前进度：

- `0 / 100` 已完成

## 001-020 Replay / Snapshots

- `[ ]` 001 transcript replay fixture: mixed assistant/tool/system/error
- `[ ]` 002 transcript replay fixture: long markdown+CJK response
- `[ ]` 003 transcript replay fixture: remote workflow handoff
- `[ ]` 004 transcript replay fixture: hook/task recovery
- `[ ]` 005 transcript replay fixture: inspector round-trip
- `[ ]` 006 generated snapshot diff budget thresholds
- `[ ]` 007 generated snapshot stale-artifact cleanup
- `[ ]` 008 snapshot catalog CI dry-run
- `[ ]` 009 snapshot docs link validation
- `[ ]` 010 snapshot fixture minimization pass
- `[ ]` 011 transcript replay fixture: subagent batch
- `[ ]` 012 transcript replay fixture: ask-user branch
- `[ ]` 013 transcript replay fixture: export bundle
- `[ ]` 014 transcript replay fixture: permission denial
- `[ ]` 015 transcript replay fixture: prompt cache break
- `[ ]` 016 snapshot section ownership map
- `[ ]` 017 snapshot output stability audit
- `[ ]` 018 snapshot ANSI/hyperlink normalization
- `[ ]` 019 snapshot CJK width baseline
- `[ ]` 020 snapshot closeout artifact

## 021-050 Visual / Renderer Guardrails

- `[ ]` 021 markdown heading visual regression
- `[ ]` 022 markdown list nesting visual regression
- `[ ]` 023 markdown table visual regression
- `[ ]` 024 markdown code fence visual regression
- `[ ]` 025 transcript latest-focus visual regression
- `[ ]` 026 grouped system visual regression
- `[ ]` 027 grouped tool visual regression
- `[ ]` 028 subagent batch visual regression
- `[ ]` 029 inspector panel visual regression
- `[ ]` 030 confirm panel visual regression
- `[ ]` 031 narrow terminal smoke test
- `[ ]` 032 medium terminal smoke test
- `[ ]` 033 wide terminal smoke test
- `[ ]` 034 hyperlink terminal smoke test
- `[ ]` 035 CJK terminal smoke test
- `[ ]` 036 color hierarchy review
- `[ ]` 037 density token audit
- `[ ]` 038 prefix/gutter audit
- `[ ]` 039 footer hint audit
- `[ ]` 040 focus/selection audit
- `[ ]` 041 scroll offset audit
- `[ ]` 042 streaming preview audit
- `[ ]` 043 code block theme audit
- `[ ]` 044 table fallback audit
- `[ ]` 045 blockquote affordance audit
- `[ ]` 046 inline code contrast audit
- `[ ]` 047 error rendering audit
- `[ ]` 048 system rendering audit
- `[ ]` 049 assistant rendering audit
- `[ ]` 050 visual guardrail closeout

## 051-080 E2E Operator Flows

- `[ ]` 051 remote review E2E
- `[ ]` 052 workflow preview/run E2E
- `[ ]` 053 workflow write-mode E2E
- `[ ]` 054 review latest E2E
- `[ ]` 055 doctor bundle E2E
- `[ ]` 056 export bundle E2E
- `[ ]` 057 artifact inspector E2E
- `[ ]` 058 permission recovery E2E
- `[ ]` 059 hook deferred E2E
- `[ ]` 060 task monitor E2E
- `[ ]` 061 task follow E2E
- `[ ]` 062 task issue E2E
- `[ ]` 063 transcript compare E2E
- `[ ]` 064 transcript picker E2E
- `[ ]` 065 memory latest E2E
- `[ ]` 066 status/diagnostics E2E
- `[ ]` 067 prompt cache E2E
- `[ ]` 068 restore diff E2E
- `[ ]` 069 coordinator timeline E2E
- `[ ]` 070 remote live session E2E
- `[ ]` 071 confirmation inspect E2E
- `[ ]` 072 confirmation explain E2E
- `[ ]` 073 confirmation deny E2E
- `[ ]` 074 confirmation always-allow E2E
- `[ ]` 075 inspector action E2E
- `[ ]` 076 inspector search E2E
- `[ ]` 077 inspector tab/focus E2E
- `[ ]` 078 update/lifecycle E2E
- `[ ]` 079 compaction boundary E2E
- `[ ]` 080 E2E closeout

## 081-100 Handoff / Governance

- `[ ]` 081 parity owner map
- `[ ]` 082 regression label taxonomy
- `[ ]` 083 release-note template refresh
- `[ ]` 084 fourth-round gap review
- `[ ]` 085 fourth-round progress checkpoint
- `[ ]` 086 renderer change checklist
- `[ ]` 087 snapshot change checklist
- `[ ]` 088 E2E change checklist
- `[ ]` 089 docs drift checklist
- `[ ]` 090 final parity signoff checklist
- `[ ]` 091 fifth-round backlog seed
- `[ ]` 092 release note draft
- `[ ]` 093 closeout review
- `[ ]` 094 handoff artifact
- `[ ]` 095 risk register refresh
- `[ ]` 096 known limitations refresh
- `[ ]` 097 CI integration proposal
- `[ ]` 098 fixture maintenance guide
- `[ ]` 099 final tracker audit
- `[ ]` 100 handoff into fifth 100 tracker
