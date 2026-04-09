# Diagnostics Lazy-Load Notes

## Principle

诊断路径尽量按需计算，不在普通 startup 流程里做重扫描。

## Current behavior

- `/status` / `/tools` / `/permissions` / `/tasks` / `/diagnostics` 都在命令执行时读取 runtime snapshot
- transcript metadata 解析走缓存
- latest transcript lookup 走缓存
- compare 超大输入直接降级，不做完整 diff

## Why this matters

- 避免长会话或大量 transcript artifact 拖慢启动
- 把重 IO 留在用户真正请求诊断时再做
