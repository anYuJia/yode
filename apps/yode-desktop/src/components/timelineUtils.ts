import { TimelineItem, SessionSummary, DesktopMessage } from "../lib/mock";
import {
  isRuntimeNoticeText,
  parseToolDetails,
  shouldHideActivityItem,
  summarizeActivityItems,
  activityGroupPreview,
  getActivityDescriptor
} from "./activity/ToolUtils";

export function normalizeProcessNoteText(text: string) {
  return text
    .replace(/[ \t]+/g, " ")
    .replace(/[ \t]+([,.;:!?，。；：！？])/g, "$1")
    .trim();
}

function processNoteFingerprint(text: string) {
  return normalizeProcessNoteText(text)
    .replace(/^(我会|我先|我来|接下来|下一步|现在|然后|先|再|接着)/, "")
    .replace(/[ \t\r\n,.;:!?，。；：！？、]/g, "")
    .trim();
}

export function looksLikeTerseToolTitle(text: string) {
  const clean = normalizeProcessNoteText(text);
  if (!clean) return true;
  if (/[，。；：！？,.!?]/.test(clean)) return false;
  if (clean.length > 10) return false;
  return /^(查看|读取|分析|获取|检查|搜索|运行|验证|整理|梳理|确认|探索)[\p{L}\p{N}_/\-. ]{0,6}$/u.test(clean);
}

export function splitProcessNotes(text: string, limit = 6) {
  return text
    .split(/\n{2,}|\n(?=(?:I will|I'll|Let me|Next|Now|我会|我先|接下来|现在|然后|下一步|先|再|接着))/i)
    .map(normalizeProcessNoteText)
    .filter((line) => line && line !== "." && line !== "..." && line !== "…")
    .slice(0, limit);
}

export function looksLikeProcessNarration(text: string) {
  const clean = normalizeProcessNoteText(text);
  if (!clean || clean.length > 520) return false;
  if (isRuntimeNoticeText(clean)) return false;
  if (/\b(the user|user hasn't|asked for|I've provided|wait for the user|user's response|want to dive deeper)\b/i.test(clean)) {
    return false;
  }
  return /^(I will|I'll|Let me|Next|Now|I need to|I’m going to|I'm going to|我会|我先|接下来|现在|然后|下一步|先|再|接着)/i.test(clean) ||
    /(读取|查看|搜索|检查|运行|验证|修改|分析|探索).*(文件|项目|代码|目录|结构|实现|结果)/i.test(clean);
}

/**
 * 处理过程旁白文本。
 * 不做任何模板翻译 —— AI 由系统提示词引导原生输出中文旁白。
 * 仅过滤掉 runtime notice 和关于用户意图的元叙述（不应展示给用户）。
 */
export function localizeProcessNoteText(text: string, appLang: string) {
  if (appLang !== "zh") return text;

  const clean = normalizeProcessNoteText(text);
  if (!clean || isRuntimeNoticeText(clean)) return "";

  // 过滤掉关于用户意图的内部分析，这类内容不适合展示
  if (/\b(the user|user hasn't|asked for|I've provided|wait for the user|user's response|want to dive deeper)\b/i.test(clean)) {
    return "";
  }

  // 原生显示 AI 返回的内容
  return clean;
}

export function localizeProcessNotes(text: string, appLang: string, limit = 6) {
  return splitProcessNotes(text, limit)
    .map((note) => localizeProcessNoteText(note, appLang))
    .filter(Boolean);
}

export function localizeVisibleProcessText(text: string, appLang: string) {
  return localizeProcessNotes(text, appLang, 12).join("\n\n");
}

export function processNote(
  id: string,
  body: string
): Extract<TimelineItem, { kind: "process_note" }> {
  return {
    id,
    kind: "process_note",
    body,
    status: "success"
  };
}

function isActionNarrativeItem(item: TimelineItem) {
  return item.kind === "process_note" && item.id.startsWith("action-narrative-");
}

function isAskUserTool(item: Extract<TimelineItem, { kind: "tool" }>) {
  const tool = (item.tool || "").toLowerCase();
  const title = (item.title || "").toLowerCase();
  return tool.includes("ask_user") || tool.includes("ask-question") || title.includes("ask_user") || title.includes("ask question");
}

function actionNarrativePrefix(turnId?: string) {
  return `action-narrative-${turnId || "current"}-`;
}

function isToolResultLikeItem(item: Extract<TimelineItem, { kind: "tool" }>) {
  if (/工具结果|tool result|result/i.test(item.title || "")) return true;
  const metadata = (item as any).metadata;
  return Boolean(
    (item as any).callId &&
    item.status === "success" &&
    metadata &&
    typeof metadata === "object" &&
    (
      metadata.activity ||
      metadata.diff_preview ||
      metadata.modified_files ||
      metadata.tool_runtime ||
      metadata.file_path ||
      metadata.path
    )
  );
}

export function syntheticNarrationForActivity(item: TimelineItem, appLang: string) {
  const isZh = appLang === "zh";
  if (!isZh) return "";

  if (item.kind === "activity_group") {
    const visibleItems = summarizeActivityItems(item.items || []);
    const preview = activityGroupPreview(visibleItems, appLang);
    if (item.type === "explore") {
      return `我先看 ${preview}，确认相关代码和渲染路径是怎么串起来的。`;
    }
    if (item.type === "search") {
      return `我会搜索 ${preview}，把可能相关的实现位置先收窄。`;
    }
    if (item.type === "run") {
      return `我会运行 ${preview}，用实际输出确认当前判断是否成立。`;
    }
    return `我会执行 ${preview}，结合返回结果决定下一步怎么处理。`;
  }

  if (item.kind === "activity_item") {
    return item.filename
      ? `我会修改 ${item.filename}，让这里的行为和 Codex 的呈现方式更接近。`
      : "我会完成这处修改，然后继续验证展示效果。";
  }

  return "";
}

export function syntheticNarrationBeforeAssistant(previous: TimelineItem | undefined, appLang: string) {
  if (appLang !== "zh") return "";
  if (!previous || (previous.kind !== "activity_group" && previous.kind !== "activity_item")) return "";
  if (previous.kind === "activity_group") {
    if (previous.type === "explore") {
      return "相关代码已经看完，我会把模型返回内容和前端模板的边界整理清楚。";
    }
    if (previous.type === "run") {
      return "验证命令已经返回，我会结合结果说明这次改动是否生效。";
    }
  }
  if (previous.kind === "activity_item") {
    return "修改已经完成，我会总结改动点和验证结果。";
  }
  return "";
}

export function isIntermediateAssistantItem(item: TimelineItem) {
  return item.kind === "assistant" && item.meta === "intermediate";
}

export function addSyntheticProcessNarration(items: TimelineItem[], appLang: string) {
  return items;
}

export function hasVisibleProcessBody(item: TimelineItem | undefined) {
  return item?.kind === "process_note" && Boolean(item.body.trim());
}

export function groupEditSummaryItems(items: TimelineItem[]): TimelineItem[] {
  const next: TimelineItem[] = [];
  let buffer: Array<Extract<TimelineItem, { kind: "activity_item" }>> = [];

  const flush = () => {
    if (buffer.length === 0) return;
    next.push({
      id: `edit-summary-${buffer[0].id}`,
      kind: "edit_summary",
      status: buffer.some((item) => item.status === "running")
        ? "running"
        : buffer.some((item) => item.status === "blocked")
          ? "blocked"
          : "success",
      items: buffer
    });
    buffer = [];
  };

  items.forEach((item) => {
    if (item.kind === "activity_item") {
      buffer.push(item);
      return;
    }
    flush();
    next.push(item);
  });
  flush();
  return next;
}

export function compileInlineItems(items: TimelineItem[], isTurnActive?: boolean, appLang = "zh"): TimelineItem[] {
  const result: TimelineItem[] = [];
  let buffer: Array<Extract<TimelineItem, { kind: "tool" }>> = [];
  const visibleActionNarrativeIndexes = new Set<number>();
  const actionNarrativeIndexes = items
    .map((item, index) => (isActionNarrativeItem(item) ? index : -1))
    .filter((index) => index >= 0);
  actionNarrativeIndexes.forEach((index) => visibleActionNarrativeIndexes.add(index));

  const getToolType = (item: Extract<TimelineItem, { kind: "tool" }>) => {
    const descriptor = getActivityDescriptor(item);
    if (descriptor.kind === "read") return "explore";
    if (descriptor.kind === "search") return "search";
    if (descriptor.kind === "edit") return "edit";
    if (descriptor.kind === "run") return "run";

    const toolName = item.tool || "";
    const title = item.title || "";
    const t = (toolName || "").toLowerCase();
    const ttl = (title || "").toLowerCase();
    if (t.includes("search") || t.includes("url") || ttl.includes("search") || ttl.includes("url") || ttl.includes("搜索") || ttl.includes("网页")) {
      return "search";
    }
    if (
      t.includes("read") || t.includes("view") || t.includes("list") || 
      t.includes("grep") || t.includes("map") || t.includes("glob") || 
      t.includes("ls") || t.includes("find") || t.includes("locate") ||
      ttl.includes("read") || ttl.includes("view") || ttl.includes("list") || 
      ttl.includes("grep") || ttl.includes("map") || ttl.includes("glob") || 
      ttl.includes("ls") || ttl.includes("find") || ttl.includes("locate") ||
      ttl.includes("探索") || ttl.includes("查看") || ttl.includes("读取")
    ) {
      return "explore";
    }
    if (
      t.includes("write") || t.includes("edit") || t.includes("replace") || 
      t.includes("create") || t.includes("patch") || t.includes("modify") ||
      ttl.includes("write") || ttl.includes("edit") || ttl.includes("replace") || 
      ttl.includes("create") || ttl.includes("patch") || ttl.includes("modify") ||
      ttl.includes("写入") || ttl.includes("修改") || ttl.includes("编辑") || ttl.includes("替换")
    ) {
      return "edit";
    }
    if (
      t.includes("run") || t.includes("command") || t.includes("bash") || 
      t.includes("execute") || t.includes("cmd") || t.includes("sh") ||
      ttl.includes("run") || ttl.includes("command") || ttl.includes("bash") || 
      ttl.includes("execute") || ttl.includes("cmd") || ttl.includes("sh") ||
      ttl.includes("运行") || ttl.includes("执行")
    ) {
      return "run";
    }
    return "other";
  };

  const flushBuffer = () => {
    if (buffer.length === 0) return;
    
    let groupType: "explore" | "search" | "run" | "mixed" | "other" = "explore";
    const tools = buffer.filter(item => item.kind === "tool");
    const visibleTools = tools.filter(item => !shouldHideActivityItem(item) && !isAskUserTool(item));
    if (visibleTools.length === 0) {
      buffer = [];
      return;
    }
    
    const runTools = visibleTools.filter(t => getToolType(t) === "run");
    const searchTools = visibleTools.filter(t => getToolType(t) === "search");
    const exploreTools = visibleTools.filter(t => getToolType(t) === "explore");
    
    const nonEmptyGroups = [runTools, searchTools, exploreTools].filter((group) => group.length > 0).length;

    if (nonEmptyGroups > 1) {
      groupType = "mixed";
    } else if (runTools.length > 0) {
      groupType = "run";
    } else if (searchTools.length > 0) {
      groupType = "search";
    } else if (exploreTools.length > 0) {
      groupType = "explore";
    } else if (visibleTools.length > 0) {
      groupType = "other";
    } else {
      groupType = "explore";
    }
    
    const isRunning = buffer.some(item => item.status === "running" || item.meta === "running");
    
    let count = 0;
    if (groupType === "explore") {
      const files = new Set<string>();
      visibleTools.forEach(t => {
        const parsed = parseToolDetails(t);
        if (parsed.filename) {
          files.add(parsed.filename);
        }
      });
      count = files.size || visibleTools.length || 1;
    } else {
      count = visibleTools.length || 1;
    }
    
    let label = "";
    if (groupType === "explore") {
      label = `${isRunning ? "正在探索" : "已探索"} ${count} 个文件`;
    } else if (groupType === "search") {
      label = `${isRunning ? "正在搜索" : "已搜索"} ${count} 次`;
    } else if (groupType === "run") {
      label = `${isRunning ? "正在运行" : "已运行"} ${count} 条命令`;
    } else if (groupType === "mixed") {
      const parts: string[] = [];
      if (exploreTools.length > 0) {
        const files = new Set<string>();
        exploreTools.forEach((item) => {
          const descriptor = getActivityDescriptor(item);
          if (descriptor.filename) files.add(descriptor.filename);
        });
        parts.push(`${isRunning ? "正在探索" : "已探索"} ${files.size || exploreTools.length} 个文件`);
      }
      if (searchTools.length > 0) parts.push(`${isRunning ? "正在搜索" : "已搜索"} ${searchTools.length} 次`);
      if (runTools.length > 0) parts.push(`${isRunning ? "正在运行" : "已运行"} ${runTools.length} 条命令`);
      const editTools = visibleTools.filter(t => getToolType(t) === "edit");
      if (editTools.length > 0) parts.push(`${isRunning ? "正在修改" : "已修改"} ${editTools.length} 个文件`);
      label = parts.length > 0 ? parts.join("") : `${isRunning ? "正在处理" : "已处理"} ${visibleTools.length || 1} 项`;
    } else {
      label = `${isRunning ? "正在执行" : "已执行"} ${count} 项`;
    }
    
    result.push({
      id: `group-${buffer[0].id}`,
      kind: "activity_group",
      type: groupType,
      status: isRunning ? "running" : "success",
      label,
      items: visibleTools
    });
    
    buffer = [];
  };

  const pushProcessNotes = (item: TimelineItem, title?: string, status?: "running" | "success") => {
    const notes = localizeProcessNotes((item as any).body || "", appLang, 20);
    if (notes.length === 0 && status !== "running") return;
    flushBuffer();
    if (notes.length === 0) {
      result.push({
        id: `${item.id}-process`,
        kind: "process_note",
        title,
        body: "",
        status: status || "success"
      });
      return;
    }
    notes.forEach((body, index) => {
      result.push({
        id: `${item.id}-process-${index}`,
        kind: "process_note",
        title: index === 0 ? title : undefined,
        body,
        status: status || "success"
      });
    });
  };

  for (let itemIndex = 0; itemIndex < items.length; itemIndex += 1) {
    const item = items[itemIndex];
    if (item.kind === "assistant") {
      const body = item.body.trim();
      const isEmpty = body === "" || body === "." || body === "..." || body === "…";
      if (!isEmpty) {
        const followedByWork = items
          .slice(itemIndex + 1)
          .some((next) => next.kind === "tool" || next.kind === "reasoning");
        if (item.meta === "intermediate" || ((isTurnActive || followedByWork) && looksLikeProcessNarration(body))) {
          pushProcessNotes(item);
        } else {
          flushBuffer();
          result.push(item);
        }
      }
    } else if (item.kind === "permission" || item.kind === "boundary") {
      flushBuffer();
      result.push(item);
    } else if (item.kind === "process_note") {
      flushBuffer();
      if (item.body.trim() || item.status === "running") {
        if (isActionNarrativeItem(item)) {
          if (!visibleActionNarrativeIndexes.has(itemIndex)) {
            continue;
          }
        }
        result.push(item);
      }
    } else if (item.kind === "tool") {
      if (isToolResultLikeItem(item)) {
        const resultCallId = (item as any).callId || item.tool;
        let found = false;

        if (resultCallId) {
          for (let i = buffer.length - 1; i >= 0; i--) {
            if (buffer[i].kind === "tool" && (buffer[i] as any).callId === resultCallId) {
              buffer[i].result = item.body;
              buffer[i].metadata = (item as any).metadata ?? (buffer[i] as any).metadata;
              buffer[i].status = item.status;
              found = true;
              break;
            }
          }
        }

        if (!found) {
          for (let i = buffer.length - 1; i >= 0; i--) {
            if (buffer[i].kind === "tool") {
              buffer[i].result = item.body;
              buffer[i].metadata = (item as any).metadata ?? (buffer[i] as any).metadata;
              buffer[i].status = item.status;
              found = true;
              break;
            }
          }
        }
        if (!found) {
          found = attachToolResultToRenderedItems(
            result,
            item.body,
            resultCallId,
            (item as any).metadata,
            item.status
          );
        }
        if (!found) {
          attachToolResultToRenderedItems(result, item.body, undefined, (item as any).metadata, item.status);
        }
        continue;
      }

      const parsed = parseToolDetails(item);
      const modifiedFiles = parsed.modifiedFiles || [];
      const type = modifiedFiles.length > 0 ? "edit" : getToolType(item);
      if (type === "edit") {
        flushBuffer();
        const files = modifiedFiles.length > 0 ? modifiedFiles : [parsed.filename];
        files.forEach((filename, index) => {
          result.push({
            id: modifiedFiles.length > 0 ? `${item.id}-modified-${index}` : item.id,
            kind: "activity_item",
            type: "edit",
            tool: item.tool,
            title: item.title,
            body: item.body,
            status: item.status,
            filename,
            diff: parsed.diff,
            callId: (item as any).callId,
            metadata: (item as any).metadata,
            result: item.result
          });
        });
      } else {
        buffer.push(item);
      }
    } else if (item.kind === "reasoning") {
      if (item.meta === "running") {
        flushBuffer();
        result.push(item);
      }
    }
  }
  flushBuffer();
  const enrichedResult = groupEditSummaryItems(addSyntheticProcessNarration(result, appLang));

  return enrichedResult;
}

function isStreamingAssistantForTurn(item: TimelineItem, turnId?: string) {
  if (item.kind !== "assistant" || item.meta !== "streaming") return false;
  if (!turnId) return true;
  return item.id.startsWith(`assistant-${turnId}-`);
}

function assistantSegmentId(turnId: string | undefined, items: TimelineItem[]) {
  if (!turnId) return `event-${Date.now()}-${Math.random()}`;
  const segmentIndex = items.filter((item) => item.id.startsWith(`assistant-${turnId}-`)).length;
  return `assistant-${turnId}-${segmentIndex}`;
}

function lastAssistantIndexForTurn(items: TimelineItem[], turnId?: string) {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    const item = items[index];
    if (item.kind !== "assistant") continue;
    if (!turnId || item.id.startsWith(`assistant-${turnId}-`)) return index;
  }
  return -1;
}

function completeStreamingAssistants(items: TimelineItem[], turnId?: string) {
  return items.map((item) => {
    if (!isStreamingAssistantForTurn(item, turnId)) return item;
    return { ...item, meta: "intermediate" };
  });
}

function hasRecentAssistantPreamble(items: TimelineItem[], turnId?: string) {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    const item = items[index];
    if (item.kind === "tool" || item.kind === "activity_group" || item.kind === "activity_item" || item.kind === "edit_summary") {
      return false;
    }
    if (item.kind !== "assistant") continue;
    if (turnId && !item.id.startsWith(`assistant-${turnId}-`)) continue;
    return Boolean(item.body.trim());
  }
  return false;
}

function attachToolResultToRenderedItems(
  items: TimelineItem[],
  resultBody: string,
  callId?: string,
  metadata?: any,
  status?: "running" | "success" | "blocked"
) {
  for (let i = items.length - 1; i >= 0; i--) {
    const resultItem = items[i];
    if (resultItem.kind === "activity_item" && resultItem.tool) {
      if (!callId || (resultItem as any).callId === callId) {
        resultItem.result = resultBody;
        resultItem.metadata = metadata ?? resultItem.metadata;
        if (status) resultItem.status = status;
        return true;
      }
    } else if (resultItem.kind === "activity_group") {
      const groupItems = resultItem.items;
      for (let j = groupItems.length - 1; j >= 0; j--) {
        const groupItem = groupItems[j];
        if (groupItem.kind === "tool" && (!callId || (groupItem as any).callId === callId)) {
          groupItem.result = resultBody;
          groupItem.metadata = metadata ?? (groupItem as any).metadata;
          if (status) groupItem.status = status;
          return true;
        }
      }
    }
  }
  return false;
}

export type ConversationTurn = {
  id: string;
  userItem: TimelineItem | null;
  items: TimelineItem[];
  hasIntermediate: boolean;
};

export function splitTurnVisibleItems(items: TimelineItem[]) {
  let finalAssistantIndex = -1;
  for (let index = items.length - 1; index >= 0; index -= 1) {
    if (isFinalAssistantItem(items[index])) {
      finalAssistantIndex = index;
      break;
    }
  }

  if (finalAssistantIndex === -1) {
    return {
      processItems: items,
      answerItems: [] as TimelineItem[]
    };
  }

  return {
    processItems: items.filter((_, index) => index !== finalAssistantIndex),
    answerItems: [items[finalAssistantIndex]]
  };
}

export function isFinalAssistantItem(item: TimelineItem) {
  return item.kind === "assistant" && !isIntermediateAssistantItem(item);
}

export function parseDurationFromTitle(title?: string) {
  if (!title) return null;
  const minuteSecond = title.match(/(\d+)\s*(?:分|m|min|分钟)\s*(\d+)?\s*(?:秒|s)?/i);
  if (minuteSecond) {
    return Number(minuteSecond[1]) * 60 + Number(minuteSecond[2] || 0);
  }
  const seconds = title.match(/(\d+)\s*(?:秒|s|sec|seconds?)/i);
  if (seconds) return Number(seconds[1]);
  return null;
}

export function turnStaticDurationSeconds(turn: ConversationTurn) {
  const hasRestoredSpan = turn.items.some((item) =>
    item.kind !== "reasoning" &&
    typeof (item as any)?.createdAt === "number" &&
    Number.isFinite((item as any).createdAt)
  );
  const createdTimes = [turn.userItem, ...turn.items]
    .map((item) => (item as any)?.createdAt)
    .filter((value): value is number => typeof value === "number" && Number.isFinite(value));

  if (hasRestoredSpan && createdTimes.length >= 2) {
    return Math.max(1, Math.round((Math.max(...createdTimes) - Math.min(...createdTimes)) / 1000));
  }

  let totalReasoningSecs = 0;
  let hasParsedReasoning = false;
  for (const item of turn.items) {
    if (item.kind === "reasoning") {
      const parsed = parseDurationFromTitle(item.title);
      if (parsed !== null) {
        totalReasoningSecs += parsed;
        hasParsedReasoning = true;
      }
    }
  }
  if (hasParsedReasoning) {
    return totalReasoningSecs;
  }

  if (createdTimes.length >= 2) {
    return Math.max(1, Math.round((Math.max(...createdTimes) - Math.min(...createdTimes)) / 1000));
  }
  return 0;
}

export function formatDurationZh(totalSeconds: number) {
  const safeSeconds = Math.max(0, Math.round(totalSeconds));
  const minutes = Math.floor(safeSeconds / 60);
  const seconds = safeSeconds % 60;
  if (minutes <= 0) return `${seconds} 秒`;
  if (seconds <= 0) return `${minutes} 分钟`;
  return `${minutes} 分 ${seconds} 秒`;
}

export function projectLabelFromPath(path: string) {
  const trimmed = path.trim();
  if (!trimmed) return "项目";
  const parts = trimmed.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] || trimmed;
}

export function deriveSessionTitle(content: string) {
  const normalized = content.replace(/\s+/g, " ").trim();
  if (!normalized) return "新对话";
  return normalized.length > 28 ? normalized.slice(0, 28) : normalized;
}

export function upsertActiveSession(items: SessionSummary[], session: SessionSummary) {
  const nextSession = { ...session, active: true };
  const exists = items.some((item) => item.id === session.id);
  if (!exists) {
    return [
      nextSession,
      ...items.map((item) => item.active ? { ...item, active: false } : item)
    ];
  }
  return items.map((item) =>
    item.id === session.id
      ? nextSession
      : item.active
        ? { ...item, active: false }
        : item
  );
}

export function formatHistoryTimestamp(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return undefined;
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  });
}

export function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

export function mergeStreamingText(current: string, incoming: string): string {
  if (!incoming) return current;
  if (!current || incoming.startsWith(current)) return incoming;
  return `${current}${incoming}`;
}

export function statusLabel(status: "running" | "success" | "blocked") {
  if (status === "running") return "运行中";
  if (status === "success") return "完成";
  return "阻塞";
}

export function parseToolCalls(raw: string | null | undefined): Array<{ id?: string; name: string; arguments: string }> {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.flatMap((item) => {
      const id = stringValue(item?.id);
      const name = stringValue(item?.name) ?? stringValue(item?.function?.name);
      const args =
        stringValue(item?.arguments) ??
        stringValue(item?.function?.arguments) ??
        JSON.stringify(item?.arguments ?? item?.function?.arguments ?? {});
      return name ? [{ id, name, arguments: args }] : [];
    });
  } catch {
    return [];
  }
}

export function desktopEventToTimelineItem(
  payload: any,
  eventKind?: string
): TimelineItem {
  const outer = payload && typeof payload === "object" && "payload" in payload ? payload : null;
  const inner = outer ? outer.payload : payload;
  const sessionId = outer ? outer.sessionId : undefined;
  const turnId = outer ? outer.turnId : undefined;

  const kind = eventKind ?? stringValue(outer?.kind) ?? stringValue(inner?.kind) ?? stringValue(inner?.type);
  const eventId = stringValue(inner?.id);
  const tool = stringValue(inner?.tool) ?? "desktop";
  const title = stringValue(inner?.title) ?? "Yode";
  const body = stringValue(inner?.body) ?? "";
  const meta = stringValue(inner?.meta);
  const status = stringValue(inner?.status);
  const metadata = inner?.metadata && typeof inner.metadata === "object" ? inner.metadata : undefined;

  const eventTime = outer?.timestamp ? new Date(outer.timestamp).getTime() : Date.now();
  const createdAt = Number.isFinite(eventTime) ? eventTime : Date.now();

  if (kind === "turn_started") {
    return {
      id: turnId ? `reasoning-${turnId}` : `event-${Date.now()}-${Math.random()}`,
      kind: "reasoning",
      title: title || "思考中",
      body: body || "",
      meta: "running",
      createdAt
      // 不在 turn_started 设置 reasoningStartedAt，
      // 而是在第一个 assistant_reasoning_delta 到达时设置，
      // 避免思考耗时包含工具执行等非思考时间
    };
  }

  if (kind === "permission" || kind === "tool_confirm_required" || kind === "plan_approval_required") {
    return {
      id: eventId ? `permission-${turnId}-${eventId}` : `event-${Date.now()}-${Math.random()}`,
      kind: "permission",
      title: title || "需要授权确认",
      body: body || `工具 "${tool}" 请求执行。`,
      tool: tool,
      risk: meta || "中等风险",
      sessionId,
      turnId,
      createdAt
    };
  }

  if (kind === "ask_user") {
    return {
      id: eventId ? `boundary-ask-${turnId}-${eventId}` : `event-${Date.now()}-${Math.random()}`,
      kind: "boundary",
      title: title || "等待用户回复",
      body: "",
      createdAt
    };
  }

  if (kind === "action_narrative") {
    return {
      id: eventId ? `action-narrative-${turnId}-${eventId}` : `event-${Date.now()}-${Math.random()}`,
      kind: "process_note",
      body,
      status: "success",
      createdAt
    };
  }

  if (kind === "tool_started" || kind === "tool_progress" || kind === "tool_result" || kind === "subagent_started" || kind === "subagent_completed" || inner?.tool) {
    return {
      id: eventId ? `tool-${turnId}-${eventId}` : `event-${Date.now()}-${Math.random()}`,
      kind: "tool",
      title,
      body,
      tool,
      status: status === "success" ? "success" : status === "blocked" ? "blocked" : "running",
      meta,
      metadata,
      createdAt
    };
  }

  if (kind === "assistant_reasoning_delta") {
    return {
      id: `event-${Date.now()}-${Math.random()}`,
      kind: "reasoning",
      title,
      body: "",
      meta,
      createdAt
    };
  }

  if (kind === "retrying") {
    return {
      id: "retrying-attempt",
      kind: "reasoning",
      title: `正在重试... (当前次数: ${inner?.attempt}/${inner?.maxAttempts || "?"})`,
      body: `下次重试倒计时: ${inner?.delaySecs || 0} 秒\n\n报错原因: ${inner?.body || ""}`,
      meta: "running"
    };
  }

  if (
    kind === "usage_update" ||
    kind === "cost_update" ||
    kind === "budget_exceeded" ||
    kind === "context_compaction_started"
  ) {
    return {
      id: `event-${Date.now()}-${Math.random()}`,
      kind: "process_note",
      title,
      body,
      status: status === "blocked" ? "success" : status === "running" ? "running" : "success"
    };
  }

  if (
    kind === "context_compressed" ||
    kind === "done" ||
    kind === "plan_mode_entered" ||
    kind === "plan_mode_exited" ||
    kind === "session_memory_updated"
  ) {
    return {
      id: `event-${Date.now()}-${Math.random()}`,
      kind: "boundary",
      title,
      body
    };
  }

  return {
    id: `event-${Date.now()}-${Math.random()}`,
    kind: "assistant",
    title,
    body,
    meta
  };
}

function toolResultImageItem(
  item: Extract<TimelineItem, { kind: "tool" }>,
  turnId?: string
): TimelineItem | null {
  const metadata = item.metadata && typeof item.metadata === "object" ? item.metadata : null;
  const imageUrl = metadata?.image_url || metadata?.imageUrl;
  if (typeof imageUrl !== "string" || !imageUrl.startsWith("data:image/")) {
    return null;
  }
  const label =
    typeof metadata?.path === "string"
      ? metadata.path.split(/[\\/]/).pop() || "image"
      : "image";
  return {
    id: `tool-image-${turnId ?? "turn"}-${item.id}`,
    kind: "assistant",
    title: "Yode",
    body: `![${label}](${imageUrl})`,
    meta: "intermediate",
    createdAt: item.createdAt
  };
}

export function applyDesktopEventToTimelineItems(
  items: TimelineItem[],
  payload: any,
  eventKind?: string
): TimelineItem[] {
  const outer = payload && typeof payload === "object" && "payload" in payload ? payload : null;
  const inner = outer ? outer.payload : payload;
  const kind = eventKind ?? stringValue(outer?.kind) ?? stringValue(inner?.kind) ?? stringValue(inner?.type);
  const title = stringValue(inner?.title) ?? "Yode";
  const body = stringValue(inner?.body) ?? "";
  const reasoning = stringValue(inner?.reasoning) ?? "";
  const turnId = stringValue(outer?.turnId);
  const reasoningId = turnId ? `reasoning-${turnId}` : undefined;
  const eventId = stringValue(inner?.id);
  const hasToolCalls = Boolean(inner?.hasToolCalls);

  if (kind === "ask_user") {
    return items;
  }

  if (kind === "action_narrative") {
    if (hasRecentAssistantPreamble(items, turnId)) {
      return items;
    }
    const nextItem = desktopEventToTimelineItem(payload, eventKind);
    if (nextItem.kind !== "process_note" || !nextItem.body.trim()) {
      return items;
    }
    const noteText = nextItem.body.trim();
    if (looksLikeTerseToolTitle(noteText)) {
      return items;
    }
    const alreadyVisible = items.some((item) => {
      if (item.kind !== "assistant" && item.kind !== "process_note") return false;
      if (isActionNarrativeItem(item)) return false;
      const visibleText = item.body.trim();
      const visibleFingerprint = processNoteFingerprint(visibleText);
      const noteFingerprint = processNoteFingerprint(noteText);
      return visibleText === noteText ||
        visibleText.includes(noteText) ||
        Boolean(noteFingerprint && visibleFingerprint.includes(noteFingerprint));
    });
    if (alreadyVisible) {
      return items;
    }
    const prefix = actionNarrativePrefix(turnId);
    const stableItem = {
      ...nextItem,
      id: `${prefix}${eventId || Date.now()}`
    };
    return [...items, stableItem];
  }

  if (kind === "tool_started" || kind === "tool_progress" || kind === "tool_result" || kind === "subagent_started" || kind === "subagent_completed") {
    const nextItem = desktopEventToTimelineItem(payload, eventKind);
    const imageItem =
      kind === "tool_result" && nextItem.kind === "tool"
        ? toolResultImageItem(nextItem, turnId)
        : null;
    const existingIndex = items.findIndex((item) => item.id === nextItem.id);
    if (existingIndex >= 0 && nextItem.kind === "tool") {
      const updated = completeStreamingAssistants(
        items.map((item, index) =>
          index === existingIndex && item.kind === "tool"
            ? {
                ...item,
                title: nextItem.title || item.title,
                body: kind === "tool_result" ? item.body : nextItem.body || item.body,
                result: kind === "tool_result" ? nextItem.body || item.result : item.result,
                status: nextItem.status,
                meta: nextItem.meta ?? item.meta,
                metadata: (nextItem as any).metadata ?? (item as any).metadata
              }
            : item
        ),
        turnId
      );
      return imageItem && !updated.some((item) => item.id === imageItem.id)
        ? [...updated, imageItem]
        : updated;
    }
    const base = [...completeStreamingAssistants(items, turnId), nextItem];
    return imageItem ? [...base, imageItem] : base;
  }

  if (kind === "turn_started") {
    const hasRunning = items.some((item) => item.kind === "reasoning" && item.meta === "running");
    if (hasRunning) {
      return items;
    }
    const nextIndex = items.filter(item => item.id.startsWith(`reasoning-${turnId}-`)).length;
    const newItem = desktopEventToTimelineItem(payload, eventKind);
    if (turnId) {
      newItem.id = `reasoning-${turnId}-${nextIndex}`;
    }
    return [...items, newItem];
  }

  if (kind === "assistant_text_delta") {
    if (!body || body === "." || body === "..." || body === "…") {
      return items;
    }
    const existingIndex = items.findIndex((item) => isStreamingAssistantForTurn(item, turnId));
    if (existingIndex >= 0) {
      return items.map((item, index) =>
        index === existingIndex && item.kind === "assistant"
          ? { ...item, body: mergeStreamingText(item.body, body), meta: "streaming" }
          : item
      );
    }
    return [
      ...items,
      {
        id: assistantSegmentId(turnId, items),
        kind: "assistant",
        title: "Yode",
        body,
        meta: "streaming",
        createdAt: Date.now()
      }
    ];
  }

  if (kind === "assistant_text_complete") {
    const existingIndex = items.findIndex((item) => isStreamingAssistantForTurn(item, turnId));
    if (existingIndex >= 0) {
      return items.map((item, index) =>
        index === existingIndex && item.kind === "assistant"
          ? { ...item, body: body || item.body, meta: "stream complete" }
          : item
      );
    }
    const lastAssistantIndex = lastAssistantIndexForTurn(items, turnId);
    if (lastAssistantIndex >= 0 && body) {
      return items.map((item, index) =>
        index === lastAssistantIndex && item.kind === "assistant"
          ? { ...item, body: body || item.body, meta: "stream complete" }
          : item
      );
    }
    if (body) {
      return [
        ...items,
        {
          id: assistantSegmentId(turnId, items),
          kind: "assistant",
          title: "Yode",
          body,
          meta: "stream complete",
          createdAt: Date.now()
        }
      ];
    }
    return items;
  }

  if (kind === "assistant_reasoning_delta") {
    const now = Date.now();
    const existingIndex = items.findIndex((item) => item.kind === "reasoning" && item.meta === "running");
    if (existingIndex >= 0) {
      return items.map((item, index) =>
        index === existingIndex && item.kind === "reasoning"
          ? {
              ...item,
              title: item.title || "正在思考...",
              meta: "running",
              body: mergeStreamingText(item.body || "", reasoning),
              reasoningStartedAt: (item as any).reasoningStartedAt || now
            }
          : item
      );
    }
    const nextIndex = items.filter(item => item.id.startsWith(`reasoning-${turnId}-`)).length;
    const newId = turnId ? `reasoning-${turnId}-${nextIndex}` : `event-${Date.now()}-${Math.random()}`;
    return [
      ...items,
      {
        id: newId,
        kind: "reasoning",
        title: "思考中...",
        body: reasoning,
        meta: "running",
        createdAt: now,
        reasoningStartedAt: now
      }
    ];
  }

  if (kind === "usage_update" || kind === "cost_update") {
    return items;
  }

  if (kind === "retrying" || kind === "context_compaction_started") {
    const nextItem = desktopEventToTimelineItem(payload, eventKind);
    if (eventId || nextItem.id === "retrying-attempt") {
      const existingIndex = items.findIndex((item) => item.id === nextItem.id);
      if (existingIndex >= 0) {
        return items.map((item, index) => index === existingIndex ? nextItem : item);
      }
    }
    return [...items, nextItem];
  }

  if (kind === "assistant_reasoning_complete") {
    const existingIndex = items.findIndex((item) => item.kind === "reasoning" && item.meta === "running");
    if (existingIndex >= 0) {
      return items.map((item, index) => {
        if (index === existingIndex && item.kind === "reasoning") {
          const start = (item as any).reasoningStartedAt;
          const duration = start ? Math.max(1, Math.round((Date.now() - start) / 1000)) : null;
          return { 
            ...item, 
            meta: "complete",
            body: reasoning || item.body,
            title: duration ? `已思考 ${duration} 秒` : "已思考"
          };
        }
        return item;
      });
    }
    const nextIndex = items.filter(item => item.id.startsWith(`reasoning-${turnId}-`)).length;
    const newId = turnId ? `reasoning-${turnId}-${nextIndex}` : `event-${Date.now()}-${Math.random()}`;
    return [
      ...items,
      {
        id: newId,
        kind: "reasoning",
        title: "已思考",
        body: reasoning,
        meta: "complete"
      }
    ];
  }

  if (kind === "turn_completed") {
    let hasAssistantForTurn = false;
    let hasReasoningForTurn = false;
    const lastAssistantIndex = lastAssistantIndexForTurn(items, turnId);
    const settledItems = items.map((item) => {
      if (item.kind === "reasoning" && turnId && item.id.startsWith(`reasoning-${turnId}-`)) {
        hasReasoningForTurn = true;
        if (item.meta === "running") {
          const start = (item as any).reasoningStartedAt;
          const duration = start ? Math.max(1, Math.round((Date.now() - start) / 1000)) : null;
          return { ...item, meta: "complete", body: reasoning || item.body, title: duration ? `已思考 ${duration} 秒` : "已思考" };
        }
      }
      if (item.kind === "tool" && item.status === "running") {
        return { ...item, status: "success" as const };
      }
      if (item.kind === "assistant" && (isStreamingAssistantForTurn(item, turnId) || Boolean(turnId && item.id.startsWith(`assistant-${turnId}-`)))) {
        hasAssistantForTurn = true;
        return {
          ...item,
          body: item.body || body,
          meta: items.indexOf(item) === lastAssistantIndex ? "stream complete" : "intermediate"
        };
      }
      if (item.kind === "assistant" && item.meta === "stream complete" && body && item.body === body) {
        hasAssistantForTurn = true;
      }
      if (item.kind === "reasoning" && reasoning && item.body === reasoning) {
        hasReasoningForTurn = true;
      }
      return item;
    });
    const fallbackItems: TimelineItem[] = [];
    if (reasoning && !hasReasoningForTurn) {
      fallbackItems.push({
        id: `event-${Date.now()}-${Math.random()}`,
        kind: "reasoning",
        title: "思考",
        body: reasoning,
        meta: "complete",
        createdAt: Date.now()
      });
    }
    if (body && !hasAssistantForTurn) {
      fallbackItems.push({
        id: `event-${Date.now()}-${Math.random()}`,
        kind: "assistant",
        title: "Yode",
        body,
        meta: hasToolCalls ? "intermediate" : "stream complete",
        createdAt: Date.now()
      });
    }
    return fallbackItems.length > 0 ? [...settledItems, ...fallbackItems] : settledItems;
  }

  if (kind === "error") {
    const filteredItems = items.filter((item) => item.id !== "retrying-attempt");
    const settledItems = filteredItems.map((item) => {
      if (item.kind === "reasoning" && item.meta === "running") {
        return { ...item, meta: "complete" };
      }
      if (item.kind === "tool" && item.status === "running") {
        return { ...item, status: "blocked" as const };
      }
      if (item.kind === "assistant" && item.meta !== "stream complete") {
        return { ...item, meta: "stream complete" };
      }
      return item;
    });

    const errorId = turnId ? `error-${turnId}` : `event-${Date.now()}-${Math.random()}`;
    const errorMessage = body || "本轮执行失败，请稍后重试。";
    const errorTitle = title && title !== "Yode" ? title : "请求失败";

    const existingIndex = settledItems.findIndex((item) => item.id === errorId);
    if (existingIndex >= 0) {
      return settledItems.map((item, index) => {
        if (index === existingIndex) {
          const existingBody = (item as any).body || "";
          let newBody = errorMessage;
          if (existingBody.includes(errorMessage)) {
            newBody = existingBody;
          } else if (errorMessage.includes(existingBody)) {
            newBody = errorMessage;
          } else {
            newBody = `${existingBody}\n${errorMessage}`;
          }
          return {
            ...item,
            kind: "error" as const,
            title: errorTitle,
            body: newBody,
            metadata: {
              ...(item as any).metadata,
              raw: newBody
            }
          };
        }
        return item;
      });
    }

    return [
      ...settledItems,
      {
        id: errorId,
        kind: "error",
        title: errorTitle,
        body: errorMessage,
        createdAt: Date.now(),
        metadata: {
          raw: errorMessage
        }
      }
    ];
  }

  return [...items, desktopEventToTimelineItem(payload, eventKind)];
}

function historyCreatedAtMs(value: string) {
  const time = new Date(value).getTime();
  return Number.isFinite(time) ? time : undefined;
}

function historyElapsedSeconds(start?: number, end?: number) {
  if (typeof start !== "number" || typeof end !== "number") return null;
  if (!Number.isFinite(start) || !Number.isFinite(end) || end < start) return null;
  return Math.max(1, Math.round((end - start) / 1000));
}

function historyReasoningTitle(turnStartMs?: number, messageCreatedAt?: number) {
  const seconds = historyElapsedSeconds(turnStartMs, messageCreatedAt);
  return seconds === null ? "已思考" : `已思考 ${seconds} 秒`;
}

function historyImageAttachments(message: DesktopMessage) {
  return (message.images ?? [])
    .filter((image) => image.base64 && image.mediaType?.startsWith("image/"))
    .map((image, index) => ({
      id: `history-${message.id}-image-${index}`,
      name: `image-${index + 1}`,
      mediaType: image.mediaType,
      base64: image.base64,
      dataUrl: `data:${image.mediaType};base64,${image.base64}`,
      size: Math.floor((image.base64.length * 3) / 4)
    }));
}

export function messagesToTimelineItems(messages: DesktopMessage[]): TimelineItem[] {
  const items: TimelineItem[] = [];
  let currentTurnStartMs: number | undefined;

  messages.forEach((message) => {
    const content = message.content?.trim();
    const reasoning = message.reasoning?.trim();
    const timestamp = formatHistoryTimestamp(message.createdAt);
    const createdAt = historyCreatedAtMs(message.createdAt);

    if (message.role === "user") {
      currentTurnStartMs = createdAt;
      const attachments = historyImageAttachments(message);
      if (content || attachments.length > 0) {
        items.push({
          id: `history-${message.id}`,
          kind: "user",
          title: "用户",
          body: content ?? "",
          attachments,
          meta: timestamp,
          createdAt
        });
      }
      return;
    }

    if (message.role === "assistant") {
      const toolCalls = parseToolCalls(message.toolCallsJson);
      if (reasoning) {
        items.push({
          id: `history-${message.id}-reasoning`,
          kind: "reasoning",
          title: historyReasoningTitle(currentTurnStartMs, createdAt),
          body: reasoning,
          meta: "complete",
          createdAt
        });
      }
      if (content) {
        const hasTools = toolCalls.length > 0;
        items.push({
          id: `history-${message.id}`,
          kind: "assistant",
          title: "Yode",
          body: content,
          meta: hasTools ? "intermediate" : "stream complete",
          createdAt
        });
      }
      toolCalls.forEach((toolCall, index) => {
        items.push({
          id: `history-${message.id}-tool-call-${index}`,
          kind: "tool",
          title: `调用工具: ${toolCall.name}`,
          body: toolCall.arguments,
          tool: toolCall.name,
          callId: toolCall.id,
          status: "success",
          meta: "history",
          createdAt
        });
      });
      return;
    }

    if (message.role === "tool") {
      items.push({
        id: `history-${message.id}`,
        kind: "tool",
        title: "工具结果",
        body: content || message.toolCallId || "",
        tool: "tool",
        callId: message.toolCallId || undefined,
        status: "success",
        metadata: message.metadata,
        createdAt
      });
      return;
    }

    return;
  });

  return items;
}
