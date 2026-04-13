# Round 5 Claude Doctor And Support Review

## Scope

这份文档对应 round-5 tracker 的 `064`。基线参考：

- `https://code.claude.com/docs/en/troubleshooting`
- `https://code.claude.com/docs/en/commands`

## Claude Baseline

- Claude Code 官方文档里的 `/doctor` 会检查安装类型、版本、搜索、配置、MCP、快捷键、plugin/agent load error、context warnings。
- 同一套命令面还有 `/debug`、`/feedback`(`/bug`)、`/status` 等 support-oriented surfaces。

## Yode Now

- Yode 已经有 `/doctor local`、`/doctor remote`、`/doctor bundle`、support handoff、bundle overview、runtime timeline、hook/recovery/permission artifacts。
- 这一套在“打包 handoff 给另一个 operator”上其实已经比纯 install checker 更完整。

## Gap

- Claude 的 install/support flow 更偏产品化入口，内建 debug/feedback 闭环更紧。
- Yode 还没有官方 issue submit、interactive install fixer、session insights 这类产品层 support affordance。

## Conclusion

- round-5 之后，Yode 在 engineering support bundle 这条线上已经很强。
- 真正差距在 product support workflow，而不是本地 doctor 数据量不足。
