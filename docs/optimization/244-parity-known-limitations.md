# Parity Known Limitations

## Terminal Variance

- Different terminals can still render color emphasis and hyperlink affordance differently.

## Snapshot Scope

- Current golden/snapshot flow is file-based and local; it is not yet backed by remote artifact storage.

## Replay Scope

- Replay CI is anchored on focused regression tests, not full transcript re-execution from serialized event logs.

## Manual Review Remainders

- Renderer changes that alter subjective readability still need human review even when snapshot and visual CI pass.
