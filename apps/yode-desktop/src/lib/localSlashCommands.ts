import { invoke } from "@tauri-apps/api/core";

import { DesktopMessage, SessionSummary, TimelineItem, UsageSnapshot } from "./desktopTypes";
import { messagesToTimelineItems, upsertActiveSession } from "./timelineUtils";
import {
  sessionsClearMessages,
  sessionsCompactEngine,
  sessionsDelete,
  sessionsExportMarkdown,
  sessionsMessages,
  sessionsRename,
  permissionModeSet
} from "./desktopIpc";

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

// ─── 统一 slash command registry ───────────────────────────────────────────
// 解析、自动补全、帮助文案、参数校验全部由同一个 registry 驱动，
// 未实现/未注册的命令不会出现在帮助文案中。

export type SlashCommandDefinition = {
  name: string;
  /** 帮助文案（zh/en）。 */
  description: (isZh: boolean) => string;
  /** 用法提示；无参数命令为 undefined。 */
  usage?: (isZh: boolean) => string;
  /** 参数校验：返回错误文案表示非法；返回 null 表示合法。 */
  validate?: (args: string, isZh: boolean) => string | null;
  execute: (args: string, context: LocalSlashCommandContext) => boolean | Promise<boolean>;
};

export function parseSlashCommand(content: string): { name: string; args: string } | null {
  if (!content.startsWith("/")) return null;
  const trimmed = content.slice(1).trim();
  const [rawName] = trimmed.split(/\s+/, 1);
  if (!rawName) return null;
  const args = trimmed.slice(rawName.length).trim();
  return { name: rawName.toLowerCase(), args };
}

/** 自动补全：返回与输入前缀匹配的已注册命令名列表。 */
export function completeSlashCommands(input: string): string[] {
  const parsed = parseSlashCommand(input);
  if (!parsed) return [];
  const prefix = parsed.name;
  if (!prefix) return [];
  return Object.keys(slashCommandRegistry)
    .filter((name) => name.startsWith(prefix))
    .sort()
    .map((name) => `/${name}`);
}

/** 帮助文案：只列出已注册（已实现）的命令。 */
export function getSlashCommandHelp(isZh: boolean): string {
  const lines = Object.values(slashCommandRegistry).map((command) => {
    const description = command.description(isZh);
    const usage = command.usage?.(isZh);
    return usage ? `${usage} - ${description}` : `/${command.name} - ${description}`;
  });
  return lines.join("\n");
}

export function validateSlashCommand(
  content: string,
  isZh: boolean
): { ok: boolean; error?: string } {
  const parsed = parseSlashCommand(content);
  if (!parsed) return { ok: true };
  const command = slashCommandRegistry[parsed.name];
  if (!command) return { ok: true };
  if (!command.validate) return { ok: true };
  const error = command.validate(parsed.args, isZh);
  return error ? { ok: false, error } : { ok: true };
}

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

export const slashCommandRegistry: Record<string, SlashCommandDefinition> = {
  new: {
    name: "new",
    description: (isZh) => (isZh ? "开启新对话" : "start a new chat"),
    execute: (_args, context) => {
      context.createSession(context.selectedProjectRoot);
      return true;
    }
  },
  clear: {
    name: "clear",
    description: (isZh) => (isZh ? "清空当前会话消息" : "clear messages in the current session"),
    execute: async (_args, context) => {
      const isZh = context.appLang === "zh";
      const append = context.appendResult;
      if (!context.activeSessionId) {
        context.createSession(context.selectedProjectRoot);
        append(
          isZh ? "已清空" : "Cleared",
          isZh ? "已开启一个新的空白对话。" : "Started a new empty chat."
        );
        return true;
      }
      try {
        await sessionsClearMessages(context.activeSessionId);
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
  },
  help: {
    name: "help",
    description: (isZh) => (isZh ? "显示命令列表" : "show this command list"),
    execute: (_args, context) => {
      context.appendResult(
        context.appLang === "zh" ? "桌面命令" : "Desktop commands",
        getSlashCommandHelp(context.appLang === "zh")
      );
      return true;
    }
  },
  "?": {
    name: "?",
    description: (isZh) => (isZh ? "显示命令列表" : "show this command list"),
    execute: (_args, context) => {
      context.appendResult(
        context.appLang === "zh" ? "桌面命令" : "Desktop commands",
        getSlashCommandHelp(context.appLang === "zh")
      );
      return true;
    }
  },
  export: {
    name: "export",
    description: (isZh) => (isZh ? "导出当前会话为 Markdown" : "export the current session as Markdown"),
    execute: async (_args, context) => {
      const isZh = context.appLang === "zh";
      const append = context.appendResult;
      if (!context.activeSessionId) {
        append(
          isZh ? "无法导出" : "Cannot export",
          isZh ? "当前还没有已保存的会话。" : "There is no saved active session yet."
        );
        return true;
      }
      try {
        const exported = await sessionsExportMarkdown(context.activeSessionId);
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
  },
  compact: {
    name: "compact",
    description: (isZh) => (isZh ? "压缩更早的会话历史" : "compact older session history"),
    execute: async (_args, context) => {
      const isZh = context.appLang === "zh";
      const append = context.appendResult;
      if (!context.activeSessionId) {
        append(
          isZh ? "无法压缩" : "Cannot compact",
          isZh ? "当前还没有已保存的会话。" : "There is no saved active session yet."
        );
        return true;
      }
      try {
        const compacted = await sessionsCompactEngine(context.activeSessionId);
        const refreshed = await sessionsMessages(context.activeSessionId);
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
  },
  permission: {
    name: "permission",
    description: (isZh) => (isZh ? "切换权限模式" : "switch permission mode"),
    usage: (isZh) => (isZh ? "/permission <default|auto|bypass|plan>" : "/permission <default|auto|bypass|plan>"),
    validate: (args, isZh) => {
      const nextMode = normalizePermissionMode(args);
      return nextMode
        ? null
        : isZh
          ? "用法：/permission default|auto|bypass|plan"
          : "Usage: /permission default|auto|bypass|plan";
    },
    execute: async (args, context) => {
      const isZh = context.appLang === "zh";
      const append = context.appendResult;
      const nextMode = normalizePermissionMode(args);
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
        await permissionModeSet({ mode: nextMode });
        context.setPermissionMode(nextMode);
        append(isZh ? "权限模式已更新" : "Permission mode updated", nextMode);
      } catch (err) {
        append(isZh ? "权限模式更新失败" : "Permission update failed", String(err));
      }
      return true;
    }
  },
  rename: {
    name: "rename",
    description: (isZh) => (isZh ? "重命名当前会话" : "rename the current session"),
    usage: (isZh) => (isZh ? "/rename <标题>" : "/rename <title>"),
    validate: (args, isZh) =>
      args.trim()
        ? null
        : isZh
          ? "用法：/rename 新标题"
          : "Usage: /rename New title",
    execute: async (args, context) => {
      const isZh = context.appLang === "zh";
      const append = context.appendResult;
      if (!context.activeSessionId) {
        append(
          isZh ? "无法重命名" : "Cannot rename",
          isZh ? "当前还没有已保存的会话。" : "There is no saved active session yet."
        );
        return true;
      }
      if (!args.trim()) {
        append(isZh ? "重命名" : "Rename", isZh ? "用法：/rename 新标题" : "Usage: /rename New title");
        return true;
      }
      try {
        const renamed = await sessionsRename(context.activeSessionId, args.trim());
        context.setSessionItems((items) => upsertActiveSession(items, renamed));
        append(isZh ? "已重命名" : "Renamed", renamed.title);
      } catch (err) {
        append(isZh ? "重命名失败" : "Rename failed", String(err));
      }
      return true;
    }
  },
  cost: {
    name: "cost",
    description: (isZh) => (isZh ? "查看最近 token 与成本统计" : "show the latest token and cost statistics"),
    execute: (_args, context) => {
      context.appendResult(
        context.appLang === "zh" ? "用量与成本" : "Usage and cost",
        formatUsageSnapshot(context.usageSnapshot, context.appLang)
      );
      return true;
    }
  },
  model: {
    name: "model",
    description: (isZh) => (isZh ? "查看当前模型" : "show the current model"),
    execute: (_args, context) => {
      context.appendResult(
        context.appLang === "zh" ? "当前模型" : "Current model",
        formatCurrentModelLabel(context.currentProvider, context.currentModel, context.appLang)
      );
      return true;
    }
  },
  sessions: {
    name: "sessions",
    description: (isZh) => (isZh ? "查看最近会话" : "show recent sessions"),
    execute: (_args, context) => {
      const isZh = context.appLang === "zh";
      const append = context.appendResult;
      const allSessions = context.sessionItems;
      const visible = allSessions.slice(0, 12);
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
  },
  status: {
    name: "status",
    description: (isZh) => (isZh ? "查看会话、模型、权限与运行状态" : "show session, model, permission mode, and run state"),
    execute: (_args, context) => {
      const isZh = context.appLang === "zh";
      const append = context.appendResult;
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
  },
  trash: {
    name: "trash",
    description: (isZh) => (isZh ? "删除当前会话并开启新对话" : "delete the current session and start a new chat"),
    execute: async (_args, context) => {
      const isZh = context.appLang === "zh";
      const append = context.appendResult;
      if (!context.activeSessionId) {
        append(
          isZh ? "无法删除" : "Cannot delete",
          isZh ? "当前还没有已保存的会话。" : "There is no saved active session yet."
        );
        return true;
      }
      if (context.sessionItems.length <= 1) {
        append(
          isZh ? "无法删除最后一个会话" : "Cannot delete the last session",
          isZh ? "请先新建一个会话后再删除当前会话。" : "Please create a new session before deleting the current one."
        );
        return true;
      }
      try {
        await sessionsDelete(context.activeSessionId);
        context.setSessionItems((items) => items.filter((s) => s.id !== context.activeSessionId));
        context.setTimelineItems([]);
        context.setUsageSnapshot(null);
        context.clearMessageQueue();
        context.setPendingUserQuestion(null);
        context.createSession(context.selectedProjectRoot);
        append(
          isZh ? "会话已删除" : "Session deleted",
          isZh ? "已删除并开启新对话。" : "Deleted and started a new chat."
        );
      } catch (err) {
        append(isZh ? "删除失败" : "Delete failed", String(err));
      }
      return true;
    }
  }
};

function normalizePermissionMode(args: string): string | null {
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
  return modeMap[args.toLowerCase()] ?? null;
}

export async function executeLocalSlashCommand(
  content: string,
  context: LocalSlashCommandContext
): Promise<boolean> {
  const parsed = parseSlashCommand(content);
  if (!parsed) return false;
  const command = slashCommandRegistry[parsed.name];
  if (!command) {
    // /review 是桌面端明确未实现的命令：不得拦截误发给 LLM 的行为，
    // 保持旧版语义（detached 审查走正常消息通道）。
    if (parsed.name === "review") return false;
    const isZh = context.appLang === "zh";
    context.appendResult(
      isZh ? "未知命令" : "Unknown command",
      isZh
        ? `桌面 app 还不支持 /${parsed.name}。输入 /help 查看当前可用命令。`
        : `The desktop app does not support /${parsed.name} yet. Type /help to see available commands.`
    );
    return true;
  }
  // 参数校验失败不得改变会话状态，也不得误发给 LLM。
  const validation = command.validate?.(parsed.args, context.appLang === "zh");
  if (validation) {
    context.appendResult(
      context.appLang === "zh" ? "参数错误" : "Invalid arguments",
      validation
    );
    return true;
  }
  return await command.execute(parsed.args, context);
}
