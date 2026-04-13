# Round 5 Runtime Orchestration Changelog

## Shipped

- workflow execution artifacts and `latest` inspector entry
- coordinator dry-run + summary artifacts
- shared runtime orchestration timeline artifact
- `/inspect artifact ...` helper for status, remote, and bundle artifacts
- workspace index cross-links for workflow/coordinator/timeline surfaces

## Notable Outcomes

- orchestration surfaces now leave durable artifacts instead of transient prompt text
- artifact freshness and stale refresh actions are visible inside inspectors
- export bundles carry the latest orchestration state with fewer redundant startup files
