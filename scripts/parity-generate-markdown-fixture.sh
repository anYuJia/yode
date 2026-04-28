#!/usr/bin/env bash
set -euo pipefail

name="${1:-markdown-cjk}"
out_dir="${2:-.yode/benchmarks/fixtures}"

mkdir -p "$out_dir"
path="$out_dir/${name}.markdown.md"

cat >"$path" <<'EOF'
# Markdown Fixture

## CJK Table

| 列 | 值 |
| --- | --- |
| 工具 | 远程 |
| 状态 | 正常 |

## Mixed Formatting

***important wrapped emphasis***

> quoted line

```rust
fn main() {}
```
EOF

echo "$path"
