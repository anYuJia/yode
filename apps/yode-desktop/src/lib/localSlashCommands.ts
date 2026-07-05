import { invoke } from "@tauri-apps/api/core";

import { DesktopMessage, SessionSummary, TimelineItem, UsageSnapshot } from "./desktopTypes";
import { messagesToTimelineItems, upsertActiveSession } from "./timelineUtils";

type SessionExportResult = {
  path: string;
  messageCount: number;
};

type SessionCompactResult = {
  beforeCount: number;
  afterCount: number;
  removedCount: number;
  summary: string;
};

export type LocalSlashCommandContext = {
  activeSession: SessionSummary | null;
  activeSessionId: string | null;
  appLang: string;
  bootstrapWorkspacePath: string;
  currentModel: string;
  currentProvider: string;
  isProcessing: boolean;
  permissionMode: string;
  selectedProjectRoot: string | null | undefined;
  sessionItems: SessionSummary[];
  timelineItemCount: number;
  usageSnapshot: UsageSnapshot | null;
  appendResult: (title: string, body: string) => void;
  createSession: (projectRoot: string | null | undefined) => void;
  clearMessageQueue: () => void;
  setPendingUserQuestion: (question: null) => void;
  setPermissionMode: (mode: string) => void;
  setSessionItems: (updater: (items: SessionSummary[]) => SessionSummary[]) => void;
  setTimelineItems: (updater: TimelineItem[] | ((items: TimelineItem[]) => TimelineItem[])) => void;
  setUsageSnapshot: (snapshot: UsageSnapshot | null) => void;
};

export function formatUsageSnapshot(snapshot: UsageSnapshot | null, appLang: string) {
  const isZh = appLang === "zh";
  if (!snapshot) {
    return isZh
      ? "当前会话还没有收到 token 或成本统计。"
      : "No token or cost statistics have been received for this session yet.";
  }
  const input = snapshot.inputTokens ?? 0;
  const output = snapshot.outputTokens ?? 0;
  const total = snapshot.totalTokens ?? input + output;
  const cacheWrite = snapshot.cacheWriteTokens ?? 0;
  const cacheRead = snapshot.cacheReadTokens ?? 0;
  const cost =
    typeof snapshot.estimatedCost === "number"
      ? `$${snapshot.estimatedCost.toFixed(4)}`
      : isZh
        ? "暂未估算"
        : "not estimated";
  return isZh
    ? `输入 ${input.toLocaleString()}，输出 ${output.toLocaleString()}，合计 ${total.toLocaleString()} tokens。缓存写入 ${cacheWrite.toLocaleString()}，缓存读取 ${cacheRead.toLocaleString()}。估算成本：${cost}。`
    : `Input ${input.toLocaleString()}, output ${output.toLocaleString()}, total ${total.toLocaleString()} tokens. Cache write ${cacheWrite.toLocaleString()}, cache read ${cacheRead.toLocaleString()}. Estimated cost: ${cost}.`;
}

export function formatCurrentModelLabel(provider: string, model: string, appLang: string) {
  const trimmedProvider = provider.trim();
  const trimmedModel = model.trim();
  if (trimmedProvider && trimmedModel) {
    return `${trimmedProvider} / ${trimmedModel}`;
  }
  if (trimmedModel) {
    return trimmedModel;
  }
  if (trimmedProvider) {
    return trimmedProvider;
  }
  return appLang === "zh" ? "未连接桌面运行时" : "Desktop runtime unavailable";
}

export async function executeLocalSlashCommand(
  content: string,
  context: LocalSlashCommandContext
) {
  if (!content.startsWith("/")) return false;
  const trimmedCommand = content.slice(1).trim();
  const [rawCommand] = trimmedCommand.split(/\s+/, 1);
  const command = rawCommand.toLowerCase();
  const commandArgs = trimmedCommand.slice(rawCommand.length).trim();
  const isZh = context.appLang === "zh";
  const append = context.appendResult;

  switch (command) {
    case "new": {
      context.createSession(context.selectedProjectRoot);
      return true;
    }
    case "clear": {
      if (!context.activeSessionId) {
        context.createSession(context.selectedProjectRoot);
        append(
          isZh ? "已清空" : "Cleared",
          isZh ? "已开启一个新的空白对话。" : "Started a new empty chat."
        );
        return true;
      }
      try {
        await invoke("sessions_clear_messages", {
          sessionId: context.activeSessionId,
          session_id: context.activeSessionId
        });
        context.setTimelineItems([]);
        context.setUsageSnapshot(null);
        context.clearMessageQueue();
        context.setPendingUserQuestion(null);
        append(
          isZh ? "已清空" : "Cleared",
          isZh ? "当前会话消息已清空。" : "The current session messages have been cleared."
        );
      } catch (err) {
        append(isZh ? "清空失败" : "Clear failed", String(err));
      }
      return true;
    }
    case "help":
    case "?": {
      append(
        isZh ? "桌面命令" : "Desktop commands",
        isZh
          ? [
              "当前可用：",
              "/clear - 清空当前会话消息",
              "/compact - 压缩较早的会话历史",
              "/export - 导出当前会话为 Markdown",
              "/new - 开启一个新对话",
              "/cost - 查看最近一次 token 与成本统计",
              "/model - 查看当前模型",
              "/permission <default|auto|bypass|plan> - 切换权限模式",
              "/rename <标题> - 重命名当前会话",
              "/sessions - 查看最近会话",
              "/status - 查看当前会话、模型、权限和运行状态",
              "/help - 显示这份命令列表",
              "",
              "更多桌面原生命令会继续补齐。"
            ].join("\n")
          : [
              "Available now:",
              "/clear - clear messages in the current session",
              "/compact - compact older session history",
              "/export - export the current session as Markdown",
              "/new - start a new chat",
              "/cost - show the latest token and cost statistics",
              "/model - show the current model",
              "/permission <default|auto|bypass|plan> - switch permission mode",
              "/rename <title> - rename the current session",
              "/sessions - show recent sessions",
              "/status - show the current session, model, permission mode, and run state",
              "/help - show this command list",
              "",
              "More native desktop commands will continue to land here."
            ].join("\n")
      );
      return true;
    }
    case "export": {
      if (!context.activeSessionId) {
        append(
          isZh ? "无法导出" : "Cannot export",
          isZh ? "当前还没有已保存的会话。" : "There is no saved active session yet."
        );
        return true;
      }
      try {
        const exported = await invoke<SessionExportResult>("sessions_export_markdown", {
          sessionId: context.activeSessionId,
          session_id: context.activeSessionId
        });
        append(
          isZh ? "会话已导出" : "Session exported",
          isZh
            ? `已导出 ${exported.messageCount} 条消息。\n${exported.path}`
            : `Exported ${exported.messageCount} messages.\n${exported.path}`
        );
      } catch (err) {
        append(isZh ? "导出失败" : "Export failed", String(err));
      }
      return true;
    }
    case "compact": {
      if (!context.activeSessionId) {
        append(
          isZh ? "无法压缩" : "Cannot compact",
          isZh ? "当前还没有已保存的会话。" : "There is no saved active session yet."
        );
        return true;
      }
      try {
        const compacted = await invoke<SessionCompactResult>("sessions_compact_engine", {
          sessionId: context.activeSessionId,
          session_id: context.activeSessionId
        });
        const refreshed = await invoke<DesktopMessage[]>("sessions_messages", {
          sessionId: context.activeSessionId,
          session_id: context.activeSessionId
        });
        context.setTimelineItems(messagesToTimelineItems(refreshed));
        append(
          compacted.removedCount > 0
            ? isZh
              ? "会话已压缩"
              : "Session compacted"
            : isZh
              ? "无需压缩"
              : "No compaction needed",
          isZh
            ? [
                `压缩前：${compacted.beforeCount} 条`,
                `压缩后：${compacted.afterCount} 条`,
                `移除：${compacted.removedCount} 条`,
                "",
                compacted.summary
              ].join("\n")
            : [
                `Before: ${compacted.beforeCount} messages`,
                `After: ${compacted.afterCount} messages`,
                `Removed: ${compacted.removedCount} messages`,
                "",
                compacted.summary
              ].join("\n")
        );
      } catch (err) {
        append(isZh ? "压缩失败" : "Compaction failed", String(err));
      }
      return true;
    }
    case "permission": {
      const normalizedMode = commandArgs.toLowerCase();
      const modeMap: Record<string, string> = {
        default: "default",
        ask: "default",
        auto: "accept-edits",
        "accept-edits": "accept-edits",
        acceptedits: "accept-edits",
        bypass: "bypass",
        trust: "bypass",
        plan: "plan"
      };
      const nextMode = modeMap[normalizedMode];
      if (!nextMode) {
        append(
          isZh ? "权限模式" : "Permission mode",
          isZh
            ? "用法：/permission default|auto|bypass|plan"
            : "Usage: /permission default|auto|bypass|plan"
        );
        return true;
      }
      try {
        await invoke("permission_mode_set", { mode: nextMode });
        context.setPermissionMode(nextMode);
        append(isZh ? "权限模式已更新" : "Permission mode updated", nextMode);
      } catch (err) {
        append(isZh ? "权限模式更新失败" : "Permission update failed", String(err));
      }
      return true;
    }
    case "rename": {
      if (!context.activeSessionId) {
        append(
          isZh ? "无法重命名" : "Cannot rename",
          isZh ? "当前还没有已保存的会话。" : "There is no saved active session yet."
        );
        return true;
      }
      if (!commandArgs) {
        append(isZh ? "重命名" : "Rename", isZh ? "用法：/rename 新标题" : "Usage: /rename New title");
        return true;
      }
      try {
        const renamed = await invoke<SessionSummary>("sessions_rename", {
          sessionId: context.activeSessionId,
          session_id: context.activeSessionId,
          title: commandArgs
        });
        context.setSessionItems((items) => upsertActiveSession(items, renamed));
        append(isZh ? "已重命名" : "Renamed", renamed.title);
      } catch (err) {
        append(isZh ? "重命名失败" : "Rename failed", String(err));
      }
      return true;
    }
    case "cost": {
      append(isZh ? "用量与成本" : "Usage and cost", formatUsageSnapshot(context.usageSnapshot, context.appLang));
      return true;
    }
    case "model": {
      append(
        isZh ? "当前模型" : "Current model",
        formatCurrentModelLabel(context.currentProvider, context.currentModel, context.appLang)
      );
      return true;
    }
    case "sessions": {
      const visible = context.sessionItems.slice(0, 12);
      const body =
        visible.length === 0
          ? isZh
            ? "暂无会话。"
            : "No sessions yet."
          : visible
              .map((session, index) => {
                const marker = session.id === context.activeSessionId ? "*" : " ";
                const model =
                  session.provider && session.model ? ` (${session.provider}/${session.model})` : "";
                return `${marker} ${index + 1}. ${session.title}${model}`;
              })
              .join("\n");
      append(isZh ? "最近会话" : "Recent sessions", body);
      return true;
    }
    case "status": {
      const project =
        context.selectedProjectRoot === null
          ? isZh
            ? "独立对话"
            : "Standalone"
          : context.selectedProjectRoot ?? context.bootstrapWorkspacePath;
      append(
        isZh ? "当前状态" : "Current status",
        isZh
          ? [
              `会话：${context.activeSession?.title ?? "新对话"}`,
              `模型：${formatCurrentModelLabel(context.currentProvider, context.currentModel, context.appLang)}`,
              `权限：${context.permissionMode}`,
              `项目：${project}`,
              `运行：${context.isProcessing ? "进行中" : "空闲"}`,
              `时间线：${context.timelineItemCount} 条`
            ].join("\n")
          : [
              `Session: ${context.activeSession?.title ?? "New chat"}`,
              `Model: ${formatCurrentModelLabel(context.currentProvider, context.currentModel, context.appLang)}`,
              `Permission: ${context.permissionMode}`,
              `Project: ${project}`,
              `Run: ${context.isProcessing ? "running" : "idle"}`,
              `Timeline: ${context.timelineItemCount} items`
            ].join("\n")
      );
      return true;
    }
    case "review":
      return false;
    default: {
      append(
        isZh ? "未知命令" : "Unknown command",
        isZh
          ? `桌面 app 还不支持 /${command}。输入 /help 查看当前可用命令。`
          : `The desktop app does not support /${command} yet. Type /help to see available commands.`
      );
      return true;
    }
  }
}
