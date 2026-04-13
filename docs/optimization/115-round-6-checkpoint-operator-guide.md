# Round 6 Checkpoint Operator Guide

1. Save a snapshot with `/checkpoint save [label]`.
2. Reopen the newest snapshot with `/checkpoint latest` or `/inspect artifact latest-checkpoint`.
3. Compare snapshots with `/checkpoint diff latest latest-1`.
4. Preview a restore without mutating runtime via `/checkpoint restore-dry-run latest`.
5. If a checkpoint looks old, trust the freshness badge before using it as the basis for a rewind or branch workflow.
