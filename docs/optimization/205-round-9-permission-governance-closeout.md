# Round 9 Permission Governance Closeout

## Scope

这份文档对应 round-9 tracker 的 `011-020`，记录 permission modes 与 governance parity 这一批的收口状态。

## Closed

- permission mode alias alignment (`dont-ask` -> closest equivalent `bypass`)
- auto mode classifier layer extracted from ad-hoc flow
- auto mode fallback / precedence chain surfacing
- category-based permission rules
- managed / user / project / local permission source views
- expanded startup permission policy artifact
- session permission governance artifact
- `/permissions governance` and `/permissions scopes`
- category-level session commands:
  - `/permissions category <name> allow`
  - `/permissions category <name> deny`
  - `/permissions category <name> ask`
- doctor / inspect surfacing for governance artifacts

## What Changed

- permission rules no longer match only by concrete tool name; they can now target category scopes such as `write`
- permission explanation now includes precedence chain rather than only a single matched rule string
- startup/bootstrap now really merges project/local permission config layers instead of flattening everything into `UserConfig`
- managed/local source badges and counts are visible in workspace/doctor surfaces
- governance artifacts are now inspectable independently from “last permission decision”

## Residual Gaps

- no enterprise network-managed control plane yet; managed config is file-backed
- `dont-ask` is currently an alias to the nearest Yode equivalent, not a fully distinct execution mode
- governance artifacts are rich, but not yet rendered as a dedicated inspector workspace family

## Conclusion

- round-9 pushed Yode permission handling from “rule engine + confirmation UI” toward a real governance plane.
- remaining gaps now concentrate in enterprise productization and presentation depth, not missing core primitives.
