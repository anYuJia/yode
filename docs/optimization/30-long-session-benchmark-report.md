# Long-Session Benchmark Report

## Scope

关注长会话下的：

- transcript metadata lookup
- latest transcript resolve
- compare diff preview
- compact artifact write
- memory truncation safety

## Current optimization points

- transcript metadata cache
- latest transcript cache
- compare size cap
- transcript/session memory write retry
- tool turn artifact summary

## Suggested benchmark set

- 100 transcript artifacts
- 500 transcript artifacts
- 1 MB compare input
- repeated `/memory list failed`
- repeated `/memory latest`
- repeated `/tools`

## Success criteria

- `/memory latest` 不再重复全量扫 metadata
- compare 在超大输入下快速降级为 summary-only
- artifact 写入遇到瞬时 IO 错误时可自动重试
