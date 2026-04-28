# Eighth CI Policy Note

Parity CI policy:

- workflow permissions remain `contents: read`
- parity jobs run with explicit `timeout-minutes`
- parity jobs share workflow concurrency by workflow/ref
- rerun entrypoint is `scripts/parity-ci-rerun.sh`
- upload artifact names stay stable per parity surface
