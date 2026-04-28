# Eighth Parity CI Integration Note

The repository CI now has explicit parity jobs for:

- `parity-snapshot`
- `parity-replay`
- `parity-visual-docs`
- snapshot drift
- replay storage
- visual hardening
- docs drift

Each job uploads a parity artifact bundle so failures have reviewable outputs instead of only console logs.
