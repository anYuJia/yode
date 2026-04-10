# Review Pipeline Cookbook

## 常见用法

### 1. 只做 review + verify

```text
Use `review_pipeline` with focus="current changes".
```

### 2. review + verify + test

```text
Use `review_pipeline` with focus="current changes" and test_command="cargo test".
```

### 3. staged-only ship

```text
/pipeline ship-staged fix auth retry path
```

这会预填一个 `review_pipeline` prompt，要求只处理 staged changes，不自动 `all=true`。

### 4. all tracked ship

```text
/pipeline ship-all release 0.2.1
```

这会预填一个 `review_pipeline` prompt，并显式设置 `all=true`。

### 5. review artifact gate

如果最近一次 review 仍是 `findings`，`/ship` 不会直接装填 commit flow，而会改成 follow-up review prompt。

## 推荐顺序

1. `/review`
2. `/pipeline test cargo test`
3. `/reviews latest`
4. `/ship <message>`

## 什么时候用 `review_then_commit`

- 简单、单步、只想做一次 review gate 再 commit

## 什么时候用 `review_pipeline`

- 需要 review + verify + optional test + optional commit 的完整串联

## 什么时候导出到 CI

```text
/pipeline export-gh
```

会生成 `.github/workflows/yode-review-gate.yml` 的起始模板。
