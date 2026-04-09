# Resume Metadata Rebuild Benchmark

## Goal

衡量 resume 场景里 transcript / memory artifact 元数据重建的开销。

## Suggested cases

- 10 transcript artifacts
- 100 transcript artifacts
- 500 transcript artifacts
- latest lookup hot cache / cold cache
- failed filter hot cache / cold cache

## Current mitigations

- transcript metadata cache
- latest transcript cache
- compare size cap

## Expected outcome

- 首次冷启动成本可接受
- 重复 `/memory latest` / `/memory list failed` 走缓存路径
