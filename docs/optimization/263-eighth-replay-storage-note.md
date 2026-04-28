# Eighth Replay Storage Note

Replay fixtures are now serialized into `.yode/benchmarks/replay/` as:

- per-fixture JSON documents
- `replay-index.json`
- `replay-index.jsonl`

This is a storage bridge between scaffolded fixture generation and future replay-from-log execution.
