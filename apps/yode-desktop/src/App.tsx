import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Archive,
  Bot,
  ChevronDown,
  ChevronRight,
  CircleDot,
  Clock3,
  Code2,
  Command,
  FileCode2,
  Folder,
  GitBranch,
  Hammer,
  History,
  KeyRound,
  MessageSquarePlus,
  MoreHorizontal,
  Paperclip,
  Pause,
  FolderPlus,
  Plus,
  Search,
  Send,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  TerminalSquare,
  Workflow,
  PanelRight,
  PanelRightClose,
  X,
  Sun,
  Moon,
  Monitor,
  Copy,
  Download,
  Hand,
  Shield,
  AlertCircle,
  Check,
  Pin,
  Trash2,
  Square
} from "lucide-react";
import React, { useCallback, useEffect, useLayoutEffect, useMemo, useState, useRef } from "react";
import { createPortal } from "react-dom";
import hljs from "highlight.js/lib/core";
// 按需注册常用语言（轻量化）
import langBash from "highlight.js/lib/languages/bash";
import langPython from "highlight.js/lib/languages/python";
import langRust from "highlight.js/lib/languages/rust";
import langTypescript from "highlight.js/lib/languages/typescript";
import langJavascript from "highlight.js/lib/languages/javascript";
import langJson from "highlight.js/lib/languages/json";
import langTOML from "highlight.js/lib/languages/ini";
import langYaml from "highlight.js/lib/languages/yaml";
import langCSS from "highlight.js/lib/languages/css";
import langHTML from "highlight.js/lib/languages/xml";
import langSQL from "highlight.js/lib/languages/sql";
import langC from "highlight.js/lib/languages/c";
import langCpp from "highlight.js/lib/languages/cpp";
import langGo from "highlight.js/lib/languages/go";
import langJava from "highlight.js/lib/languages/java";
import langMarkdown from "highlight.js/lib/languages/markdown";
import langDiff from "highlight.js/lib/languages/diff";
hljs.registerLanguage("bash", langBash);
hljs.registerLanguage("sh", langBash);
hljs.registerLanguage("shell", langBash);
hljs.registerLanguage("zsh", langBash);
hljs.registerLanguage("python", langPython);
hljs.registerLanguage("py", langPython);
hljs.registerLanguage("rust", langRust);
hljs.registerLanguage("rs", langRust);
hljs.registerLanguage("typescript", langTypescript);
hljs.registerLanguage("ts", langTypescript);
hljs.registerLanguage("tsx", langTypescript);
hljs.registerLanguage("javascript", langJavascript);
hljs.registerLanguage("js", langJavascript);
hljs.registerLanguage("jsx", langJavascript);
hljs.registerLanguage("json", langJson);
hljs.registerLanguage("toml", langTOML);
hljs.registerLanguage("ini", langTOML);
hljs.registerLanguage("yaml", langYaml);
hljs.registerLanguage("yml", langYaml);
hljs.registerLanguage("css", langCSS);
hljs.registerLanguage("html", langHTML);
hljs.registerLanguage("xml", langHTML);
hljs.registerLanguage("sql", langSQL);
hljs.registerLanguage("c", langC);
hljs.registerLanguage("cpp", langCpp);
hljs.registerLanguage("go", langGo);
hljs.registerLanguage("java", langJava);
hljs.registerLanguage("md", langMarkdown);
hljs.registerLanguage("markdown", langMarkdown);
hljs.registerLanguage("diff", langDiff);

import {
  Bootstrap,
  DesktopEvent,
  DesktopMessage,
  fallbackBootstrap,
  SessionSummary,
  sessions,
  TimelineItem,
  timeline,
  TurnAccepted
} from "./lib/mock";
import { SettingsShell } from "./components/SettingsShell";
import { TerminalDrawer } from "./components/TerminalDrawer";
import { PROVIDERS_META } from "./components/settings/ProvidersSettings";

type ViewMode = "chat" | "settings";
type PendingUserQuestion = {
  sessionId: string;
  turnId: string;
  question: string;
};

const PROJECT_ROOTS_STORAGE_KEY = "yode-project-roots";
const PROJECT_ORDER_STORAGE_KEY = "yode-project-order";
const SELECTED_PROJECT_ROOT_STORAGE_KEY = "yode-selected-project-root";
const ARCHIVED_SESSION_IDS_STORAGE_KEY = "yode-archived-session-ids";
const DELETED_SESSION_IDS_STORAGE_KEY = "yode-deleted-session-ids";
const STANDALONE_PROJECT_SENTINEL = "__standalone__";

function loadStoredProjectRoots(): string[] {
  try {
    const raw = localStorage.getItem(PROJECT_ROOTS_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return dedupeProjectRoots(parsed.filter((value): value is string => typeof value === "string"));
  } catch {
    return [];
  }
}

function loadStoredProjectOrder(): string[] {
  try {
    const raw = localStorage.getItem(PROJECT_ORDER_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return dedupeProjectRoots(parsed.filter((value): value is string => typeof value === "string"));
  } catch {
    return [];
  }
}

function loadStoredSelectedProjectRoot(): string | null | undefined {
  const raw = localStorage.getItem(SELECTED_PROJECT_ROOT_STORAGE_KEY);
  if (raw === null) return undefined;
  return raw === STANDALONE_PROJECT_SENTINEL ? null : raw;
}

function normalizeProjectRoot(root: string | null | undefined) {
  const trimmed = root?.trim();
  return trimmed ? trimmed : null;
}

function dedupeProjectRoots(roots: Array<string | null | undefined>) {
  const seen = new Set<string>();
  const unique: string[] = [];
  roots.forEach((root) => {
    const normalized = normalizeProjectRoot(root);
    if (!normalized || seen.has(normalized)) return;
    seen.add(normalized);
    unique.push(normalized);
  });
  return unique;
}

function loadStoredStringArray(key: string): string[] {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((value): value is string => typeof value === "string");
  } catch {
    return [];
  }
}

function visibleSessions(sessions: SessionSummary[]) {
  const hiddenIds = new Set([
    ...loadStoredStringArray(ARCHIVED_SESSION_IDS_STORAGE_KEY),
    ...loadStoredStringArray(DELETED_SESSION_IDS_STORAGE_KEY),
  ]);
  return sessions.filter((session) => !hiddenIds.has(session.id));
}

interface AgentAction {
  id: string;
  type: "explore" | "edit" | "run" | "reasoning";
  label: string;
  items: TimelineItem[];
}

function compileTurnActions(items: TimelineItem[]): AgentAction[] {
  const actions: AgentAction[] = [];
  const toolsRun = items.filter((item): item is Extract<TimelineItem, { kind: "tool" }> => item.kind === "tool");
  
  // Helper to match tools
  const isExploreTool = (toolName: string, title: string) => {
    const t = (toolName || "").toLowerCase();
    const ttl = (title || "").toLowerCase();
    return t.includes("read") || t.includes("view") || t.includes("list") || 
           t.includes("grep") || t.includes("map") || t.includes("glob") || 
           t.includes("ls") || t.includes("find") || t.includes("locate") ||
           ttl.includes("read") || ttl.includes("view") || ttl.includes("list") || 
           ttl.includes("grep") || ttl.includes("map") || ttl.includes("glob") || 
           ttl.includes("ls") || ttl.includes("find") || ttl.includes("locate") ||
           ttl.includes("探索") || ttl.includes("查看") || ttl.includes("搜索") || ttl.includes("读取");
  };

  const isEditTool = (toolName: string, title: string) => {
    const t = (toolName || "").toLowerCase();
    const ttl = (title || "").toLowerCase();
    return t.includes("write") || t.includes("edit") || t.includes("replace") || 
           t.includes("create") || t.includes("patch") || t.includes("modify") ||
           ttl.includes("write") || ttl.includes("edit") || ttl.includes("replace") || 
           ttl.includes("create") || ttl.includes("patch") || ttl.includes("modify") ||
           ttl.includes("写入") || ttl.includes("修改") || ttl.includes("编辑") || ttl.includes("替换");
  };

  const isRunTool = (toolName: string, title: string) => {
    const t = (toolName || "").toLowerCase();
    const ttl = (title || "").toLowerCase();
    return t.includes("run") || t.includes("command") || t.includes("bash") || 
           t.includes("execute") || t.includes("cmd") || t.includes("sh") ||
           ttl.includes("run") || ttl.includes("command") || ttl.includes("bash") || 
           ttl.includes("execute") || ttl.includes("cmd") || ttl.includes("sh") ||
           ttl.includes("运行") || ttl.includes("执行");
  };

  // 1. Thought / Reasoning
  const hasReasoning = items.some(item => item.kind === "reasoning");
  const isThinking = items.some(item => item.kind === "reasoning" && (item as any).meta === "running");
  if (hasReasoning) {
    const reasoningItems = items.filter(item => item.kind === "reasoning");
    const completedItem = reasoningItems.find(item => (item as any).meta !== "running");
    let label = "已思考";
    if (isThinking) {
      label = "正在思考...";
    } else if (completedItem) {
      label = (completedItem as any).title || "已思考";
    }
    actions.push({
      id: "reasoning",
      type: "reasoning",
      label,
      items: reasoningItems
    });
  }

  // 2. Exploration
  const readTools = toolsRun.filter(item => isExploreTool(item.tool || "", item.title || ""));
  if (readTools.length > 0) {
    const isRunning = readTools.some(item => item.status === "running");
    actions.push({
      id: "explore",
      type: "explore",
      label: isRunning ? "正在探索..." : "正在探索",
      items: readTools
    });
  }

  // 3. Edits
  const writeTools = toolsRun.filter(item => isEditTool(item.tool || "", item.title || ""));
  if (writeTools.length > 0) {
    const isRunning = writeTools.some(item => item.status === "running");
    actions.push({
      id: "edit",
      type: "edit",
      label: isRunning ? "正在修改..." : "正在修改",
      items: writeTools
    });
  }

  // 4. Commands
  const runTools = toolsRun.filter(item => isRunTool(item.tool || "", item.title || ""));
  if (runTools.length > 0) {
    const isRunning = runTools.some(item => item.status === "running");
    actions.push({
      id: "run",
      type: "run",
      label: isRunning ? "正在运行..." : "正在运行",
      items: runTools
    });
  }

  // 5. Fallback for other tools (if not matched above)
  const otherTools = toolsRun.filter(item => 
    !isExploreTool(item.tool || "", item.title || "") && 
    !isEditTool(item.tool || "", item.title || "") && 
    !isRunTool(item.tool || "", item.title || "")
  );
  if (otherTools.length > 0) {
    const isRunning = otherTools.some(item => item.status === "running");
    actions.push({
      id: "other_tools",
      type: "run",
      label: isRunning ? "正在执行..." : "正在执行",
      items: otherTools
    });
  }

  // 6. Total tool usage count
  const uniqueToolCalls = toolsRun.filter(item => item.title !== "工具结果");
  if (uniqueToolCalls.length > 0) {
    actions.push({
      id: "total_tools",
      type: "run",
      label: `工具调用次数: ${uniqueToolCalls.length}`,
      items: uniqueToolCalls
    });
  }

  return actions;
}

function getFileIcon(filename: string) {
  const ext = filename.split('.').pop()?.toLowerCase();
  switch (ext) {
    case "tsx":
    case "jsx":
      return <span style={{ color: "#61dafb", marginRight: "4px" }}>⚛️</span>;
    case "ts":
    case "js":
      return <span style={{ color: "#3178c6", marginRight: "4px" }}>📄</span>;
    case "rs":
      return <span style={{ color: "#dea584", marginRight: "4px" }}>🦀</span>;
    case "json":
      return <span style={{ color: "#cbcb41", marginRight: "4px" }}>⚙️</span>;
    case "css":
    case "scss":
      return <span style={{ color: "#563d7c", marginRight: "4px" }}>🎨</span>;
    case "md":
      return <span style={{ color: "#858585", marginRight: "4px" }}>📝</span>;
    default:
      return <span style={{ color: "#858585", marginRight: "4px" }}>📄</span>;
  }
}

function isRuntimeNoticeText(text?: string) {
  if (!text) return false;
  return /limit instead of re-reading|budget notice|checkpoint:|tool calls used|summariz(?:e|ing) current findings|most efficient next step/i.test(text);
}

function parseToolDetails(item: { tool: string; body: string; title: string }) {
  let filename = "";
  let lineRange = "";
  let diff = "";
  let command = "";

  const body = (item.body || "").trim();
  const title = (item.title || "").trim();

  if (isRuntimeNoticeText(body) || isRuntimeNoticeText(title)) {
    return { filename, lineRange, diff, command };
  }

  try {
    const parsed = JSON.parse(body);
    const rawPath = parsed.file_path || parsed.TargetFile || parsed.AbsolutePath || parsed.Path || parsed.Target || parsed.SearchPath || parsed.TargetContentFile;
    if (rawPath && typeof rawPath === "string") {
      filename = rawPath.substring(rawPath.lastIndexOf('/') + 1);
    }

    const start = parsed.StartLine;
    const end = parsed.EndLine;
    if (start !== undefined && end !== undefined) {
      lineRange = `#L${start}-${end}`;
    } else if (start !== undefined) {
      lineRange = `#L${start}`;
    }

    if (parsed.CommandLine) {
      command = parsed.CommandLine;
    }

    if (item.tool?.includes("replace") || item.tool?.includes("write") || item.tool?.includes("edit")) {
      const target = parsed.TargetContent || parsed.targetContent || "";
      const replacement = parsed.ReplacementContent || parsed.replacementContent || parsed.CodeContent || parsed.codeContent || "";
      if (target || replacement) {
        const targetLines = target ? target.split("\n").length : 0;
        const replacementLines = replacement ? replacement.split("\n").length : 0;
        diff = `+${replacementLines} -${targetLines}`;
      }
    }
  } catch (e) {
    const pathMatch = body.match(/"(?:file_path|AbsolutePath|TargetFile|Path|SearchPath)"\s*:\s*"([^"]+)"/);
    if (pathMatch) {
      const rawPath = pathMatch[1];
      filename = rawPath.substring(rawPath.lastIndexOf('/') + 1);
    }

    const startMatch = body.match(/"StartLine"\s*:\s*(\d+)/);
    const endMatch = body.match(/"EndLine"\s*:\s*(\d+)/);
    if (startMatch && endMatch) {
      lineRange = `#L${startMatch[1]}-${endMatch[1]}`;
    } else if (startMatch) {
      lineRange = `#L${startMatch[1]}`;
    }

    const cmdMatch = body.match(/"CommandLine"\s*:\s*"([^"]+)"/);
    if (cmdMatch) {
      command = cmdMatch[1];
    }

    if (item.tool?.includes("replace") || item.tool?.includes("write") || item.tool?.includes("edit")) {
      const targetMatch = body.match(/"(?:TargetContent|targetContent)"\s*:\s*"([\s\S]*?)"/);
      const replacementMatch = body.match(/"(?:ReplacementContent|replacementContent|CodeContent|codeContent)"\s*:\s*"([\s\S]*?)"/);
      if (targetMatch || replacementMatch) {
        const target = targetMatch ? targetMatch[1] : "";
        const replacement = replacementMatch ? replacementMatch[1] : "";
        const targetLines = target ? target.split("\\n").length : 0;
        const replacementLines = replacement ? replacement.split("\\n").length : 0;
        diff = `+${replacementLines} -${targetLines}`;
      }
    }
  }

  if (!filename && (item.tool?.includes("view") || item.tool?.includes("read") || item.tool?.includes("grep"))) {
    if (body && !body.startsWith('{') && (body.includes('/') || body.includes('\\') || body.includes('.'))) {
      filename = body.substring(Math.max(body.lastIndexOf('/'), body.lastIndexOf('\\')) + 1);
    }
  }

  if (!command && (item.tool?.includes("run") || item.tool?.includes("command") || item.tool?.includes("bash"))) {
    if (body && !body.startsWith('{') && looksLikeShellCommand(body)) {
      command = body;
    }
  }

  if (!filename && title) {
    const parts = item.title.split(/[\s/\\]+/);
    const lastPart = parts[parts.length - 1];
    if (lastPart && lastPart.includes(".") && !lastPart.includes("]")) {
      filename = lastPart;
    }
  }

  return { filename, lineRange, diff, command };
}

function displayToolName(tool?: string) {
  const name = (tool || "").trim();
  if (!name) return "工具";
  if (name === "project_map") return "项目结构";
  if (name === "glob") return "文件匹配";
  if (name === "grep" || name === "rg") return "内容搜索";
  if (name === "ls") return "目录列表";
  if (name === "tauri command") return "桌面命令";
  return name;
}

function looksLikeShellCommand(text: string) {
  const clean = text.trim();
  if (!clean || clean.length > 160) return false;
  if (/[\u4e00-\u9fff]/.test(clean)) return false;
  return /^(cargo|pnpm|npm|yarn|bun|git|rg|grep|find|ls|cat|sed|awk|bash|zsh|sh|python|node|deno|make|cmake|go|rustc|tsc|vite)\b/.test(clean) ||
    /(\s&&\s|\s\|\s|\s;\s|^\.\/|^\w+=\S+\s+\w+)/.test(clean);
}

function shouldHideActivityItem(item: any) {
  return isRuntimeNoticeText(item?.title) || isRuntimeNoticeText(item?.body) || isRuntimeNoticeText(item?.result);
}

function isThinkingStatusTitle(title?: string) {
  return /思考|thinking|thought/i.test(title || "");
}

function summarizeActivityItems(items: any[]) {
  const summarized: any[] = [];
  const seen = new Map<string, any>();

  for (const item of items) {
    if (shouldHideActivityItem(item)) continue;

    if (item.kind !== "tool") {
      summarized.push(item);
      continue;
    }

    const parsed = parseToolDetails(item);
    const key = [
      item.kind,
      item.tool || "",
      parsed.filename || parsed.command || "",
    ].join(":");

    const existing = seen.get(key);
    if (existing) {
      existing.count = (existing.count || 1) + 1;
      if (item.status === "running") existing.status = "running";
      if (!existing.result && item.result) existing.result = item.result;
      continue;
    }

    const next = { ...item, count: 1 };
    seen.set(key, next);
    summarized.push(next);
  }

  return summarized;
}

function activityItemSummary(item: any) {
  if (item.kind !== "tool") return "";
  const parsed = parseToolDetails(item);
  if (parsed.filename) return parsed.filename;
  if (parsed.command) return parsed.command;
  return displayToolName(item.tool);
}

function activityGroupPreview(items: any[], appLang: string) {
  const isZh = appLang === "zh";
  const labels: string[] = [];

  for (const item of items) {
    const label = activityItemSummary(item);
    if (label && !labels.includes(label)) labels.push(label);
    if (labels.length >= 4) break;
  }

  if (labels.length === 0) {
    return isZh ? "点击展开查看活动明细" : "Expand to view activity details";
  }

  const suffix = items.length > labels.length
    ? (isZh ? ` 等 ${items.length} 项` : ` and ${items.length - labels.length} more`)
    : "";
  return `${labels.join("、")}${suffix}`;
}

function ActivityLeafNode({ item, appLang }: { item: any; appLang: string }) {
  const isZh = appLang === "zh";
  const [isExpanded, setIsExpanded] = useState(false);

  const hasBodyOrResult = !!(item.body || item.result);

  if (item.kind === "reasoning") {
    let displayTitle = item.title || "";
    if (displayTitle.includes("已思考")) {
      const match = displayTitle.match(/\d+/);
      const seconds = match ? match[0] : "0";
      displayTitle = isZh ? `思考了 ${seconds} 秒` : `Thought for ${seconds}s`;
    } else if (item.meta === "running") {
      displayTitle = isZh ? "正在思考..." : "Thinking...";
    }
    
    return (
      <div style={{ display: "flex", flexDirection: "column" }}>
        <div 
          onClick={() => item.body && setIsExpanded(!isExpanded)}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "4px",
            cursor: item.body ? "pointer" : "default",
            color: "var(--text-soft)",
            fontSize: "12px"
          }}
        >
          <span>{displayTitle}</span>
          {item.body && (
            isExpanded ? <ChevronDown size={11} style={{ opacity: 0.6 }} /> : <ChevronRight size={11} style={{ opacity: 0.6 }} />
          )}
        </div>
        {isExpanded && item.body && (
          <div style={{
            marginTop: "4px",
            padding: "8px 12px",
            background: "color-mix(in oklch, var(--field), transparent 2%)",
            borderRadius: "6px",
            fontSize: "11px",
            color: "var(--text-soft)",
            whiteSpace: "pre-wrap",
            fontFamily: "var(--font-code)",
            border: "1px solid var(--line-soft)",
            maxWidth: "600px"
          }}>
            {item.body}
          </div>
        )}
      </div>
    );
  }

  if (item.kind === "tool") {
    const parsed = parseToolDetails(item);
    const isRunning = item.status === "running";
    
    let label = "";
    if (item.tool?.includes("view") || item.tool?.includes("read") || item.tool?.includes("grep") || item.tool?.includes("glob") || item.tool?.includes("list")) {
      label = isRunning 
        ? (isZh ? "正在分析" : "Analyzing") 
        : (isZh ? "已分析" : "Analyzed");
    } else if (item.tool?.includes("run") || item.tool?.includes("command") || item.tool?.includes("bash")) {
      label = isRunning 
        ? (isZh ? "正在运行命令" : "Running command") 
        : (isZh ? "已运行命令" : "Ran command");
    } else {
      label = isRunning
        ? (isZh ? "正在执行" : "Executing")
        : (isZh ? "已执行" : "Executed");
    }

    return (
      <div style={{ display: "flex", flexDirection: "column" }}>
        <div 
          onClick={() => hasBodyOrResult && setIsExpanded(!isExpanded)}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "4px",
            cursor: hasBodyOrResult ? "pointer" : "default",
            color: "var(--text-soft)",
            fontSize: "12px"
          }}
        >
          <span>{label}</span>
          {parsed.filename && getFileIcon(parsed.filename)}
          {(parsed.filename || parsed.command) ? (
            <span style={{ color: "var(--text)", fontWeight: "500" }}>
              {parsed.filename ? `${parsed.filename}${parsed.lineRange}` : parsed.command}
            </span>
          ) : (
            <span style={{ color: "var(--text)", fontWeight: "500" }}>
              {displayToolName(item.tool)}
            </span>
          )}
          {item.count > 1 && (
            <span style={{ color: "var(--text-soft)", fontSize: "11px" }}>x{item.count}</span>
          )}
          {hasBodyOrResult && (
            isExpanded ? <ChevronDown size={11} style={{ opacity: 0.6 }} /> : <ChevronRight size={11} style={{ opacity: 0.6 }} />
          )}
        </div>
        
        {isExpanded && (
          <div style={{
            marginTop: "4px",
            display: "flex",
            flexDirection: "column",
            gap: "6px",
            paddingLeft: "10px",
            borderLeft: "1.5px solid var(--line-soft)",
            maxWidth: "600px"
          }}>
            {item.body && (
              <div>
                <pre style={{
                  margin: 0,
                  padding: "6px 10px",
                  background: "color-mix(in oklch, var(--field), transparent 4%)",
                  borderRadius: "4px",
                  overflowX: "auto",
                  fontFamily: "var(--font-code)",
                  fontSize: "11px",
                  color: "var(--text-soft)",
                  border: "1px solid var(--line-soft)"
                }}>
                  {item.body}
                </pre>
              </div>
            )}
            {item.result && (
              <div>
                <pre style={{
                  margin: 0,
                  padding: "6px 10px",
                  background: "color-mix(in oklch, var(--field), transparent 2%)",
                  borderRadius: "4px",
                  overflowX: "auto",
                  maxHeight: "150px",
                  fontFamily: "var(--font-code)",
                  fontSize: "11px",
                  color: "var(--text-muted)",
                  border: "1px solid var(--line-soft)"
                }}>
                  {item.result}
                </pre>
              </div>
            )}
          </div>
        )}
      </div>
    );
  }

  return null;
}

function ActivityGroupNode({ group, appLang, isTurnActive }: { group: any; appLang: string; isTurnActive?: boolean }) {
  const isZh = appLang === "zh";
  const visibleItems = useMemo(() => summarizeActivityItems(group.items || []), [group.items]);
  const isRunning = group.status === "running";
  const shouldAutoExpand = visibleItems.length > 0 && visibleItems.length <= 4;
  const [isExpanded, setIsExpanded] = useState(shouldAutoExpand);

  useEffect(() => {
    setIsExpanded(shouldAutoExpand);
  }, [group.id, shouldAutoExpand]);

  if (visibleItems.length === 0 && !isRunning) return null;

  const count = visibleItems.filter((item: any) => item.kind === "tool").length || 1;
  const displayedItems = isExpanded && visibleItems.length > 8
    ? [...visibleItems.slice(0, 4), ...visibleItems.slice(-3)]
    : visibleItems;
  const hiddenCount = isExpanded && visibleItems.length > displayedItems.length
    ? visibleItems.length - displayedItems.length
    : 0;
  
  let label = group.label;
  if (group.type === "explore") {
    const files = new Set<string>();
    visibleItems.forEach((t: any) => {
      if (t.kind === "tool") {
        const parsed = parseToolDetails(t);
        if (parsed.filename) files.add(parsed.filename);
      }
    });
    const uniqueFilesCount = files.size;
    if (isZh) {
      label = uniqueFilesCount > 0
        ? (isRunning ? `正在探索 ${uniqueFilesCount} 个文件` : `已探索 ${uniqueFilesCount} 个文件`)
        : (isRunning ? `正在探索 ${count} 项` : `已探索 ${count} 项`);
    } else {
      label = uniqueFilesCount > 0
        ? (isRunning ? `Exploring ${uniqueFilesCount} file${uniqueFilesCount > 1 ? "s" : ""}` : `Explored ${uniqueFilesCount} file${uniqueFilesCount > 1 ? "s" : ""}`)
        : (isRunning ? `Exploring ${count} item${count > 1 ? "s" : ""}` : `Explored ${count} item${count > 1 ? "s" : ""}`);
    }
  } else if (group.type === "search") {
    if (isZh) {
      label = isRunning ? `正在搜索网页...` : `已搜索网页 ${count} 次`;
    } else {
      label = isRunning ? `Searching web...` : `Searched web ${count} time${count > 1 ? "s" : ""}`;
    }
  } else if (group.type === "run") {
    if (isZh) {
      label = isRunning ? `正在运行 ${count} 个命令` : `已运行 ${count} 个命令`;
    } else {
      label = isRunning ? `Running ${count} command${count > 1 ? "s" : ""}` : `Ran ${count} command${count > 1 ? "s" : ""}`;
    }
  } else {
    if (isZh) {
      label = isRunning ? `正在执行...` : `已执行 ${count} 个操作`;
    } else {
      label = isRunning ? `Working...` : `Executed ${count} action${count > 1 ? "s" : ""}`;
    }
  }

  return (
    <div style={{
      maxWidth: "760px",
      width: "100%",
      margin: "4px auto 8px",
      paddingLeft: "33px",
      fontSize: "12.5px",
      color: "var(--text-soft)",
      userSelect: "none"
    }}>
      <div 
        onClick={() => setIsExpanded(!isExpanded)}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "6px",
          cursor: "pointer",
          transition: "color 0.15s ease",
          fontWeight: "500",
        }}
        onMouseEnter={(e) => { e.currentTarget.style.color = "var(--text)"; }}
        onMouseLeave={(e) => { e.currentTarget.style.color = "var(--text-soft)"; }}
      >
        <span>{label}</span>
        {isExpanded ? <ChevronDown size={12} style={{ opacity: 0.8 }} /> : <ChevronRight size={12} style={{ opacity: 0.8 }} />}
      </div>

      {!isExpanded && visibleItems.length > 0 && (
        <div style={{
          marginTop: "5px",
          paddingLeft: "16px",
          color: "var(--text)",
          fontSize: "12px",
          lineHeight: 1.45,
          maxWidth: "68ch"
        }}>
          {activityGroupPreview(visibleItems, appLang)}
        </div>
      )}

      {isExpanded && (
        <div style={{
          marginTop: "6px",
          paddingLeft: "16px",
          display: "flex",
          flexDirection: "column",
          gap: "6px",
        }}>
          {displayedItems.map((item: any, idx: number) => (
            <ActivityLeafNode key={idx} item={item} appLang={appLang} />
          ))}
          {hiddenCount > 0 && (
            <div style={{ color: "var(--text-soft)", fontSize: "12px" }}>
              {isZh ? `已折叠 ${hiddenCount} 条重复/低优先级活动` : `${hiddenCount} repeated or low-priority activities hidden`}
            </div>
          )}
          {isRunning && (() => {
            const runningItem = visibleItems.find((item: any) => item.status === "running" || item.meta === "running");
            let statusText = isZh ? "正在处理..." : "Working..";
            if (runningItem) {
              if (runningItem.kind === "reasoning") {
                statusText = isZh ? "正在思考..." : "Thinking...";
              } else if (runningItem.kind === "tool") {
                const parsed = parseToolDetails(runningItem);
                if (runningItem.tool?.includes("run") || runningItem.tool?.includes("command") || runningItem.tool?.includes("bash")) {
                  statusText = parsed.command
                    ? (isZh ? `正在运行命令 ${parsed.command}...` : `Running command ${parsed.command}...`)
                    : (isZh ? "正在运行命令..." : "Running command...");
                } else if (parsed.filename) {
                  statusText = isZh ? `正在分析 ${parsed.filename}...` : `Analyzing ${parsed.filename}...`;
                } else {
                  statusText = isZh ? `正在执行 ${displayToolName(runningItem.tool)}...` : `Executing ${displayToolName(runningItem.tool)}...`;
                }
              }
            }
            return (
              <div style={{ display: "flex", alignItems: "center", gap: "6px", color: "var(--accent)", fontSize: "12px", fontStyle: "italic" }}>
                <CircleDot size={10} className="glowing-logo" style={{ animation: "pulse 2s infinite" }} />
                <span>{statusText}</span>
              </div>
            );
          })()}
        </div>
      )}
    </div>
  );
}

function ActivityItemNode({ node, appLang }: { node: any; appLang: string }) {
  const isZh = appLang === "zh";
  const [isExpanded, setIsExpanded] = useState(false);

  const isRunning = node.status === "running";
  const label = isRunning 
    ? (isZh ? "正在修改" : "Editing") 
    : (isZh ? "已修改" : "Edited");

  let addCount = "";
  let delCount = "";
  if (node.diff) {
    const parts = node.diff.split(" ");
    addCount = parts[0] || "";
    delCount = parts[1] || "";
  }

  return (
    <div style={{
      maxWidth: "760px",
      width: "100%",
      margin: "4px auto 8px",
      paddingLeft: "33px",
      fontSize: "12.5px",
      color: "var(--text-soft)",
      userSelect: "none"
    }}>
      <div 
        onClick={() => node.body && setIsExpanded(!isExpanded)}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "6px",
          cursor: node.body ? "pointer" : "default",
          transition: "color 0.15s ease",
          fontWeight: "500",
        }}
        onMouseEnter={(e) => { if (node.body) e.currentTarget.style.color = "var(--text)"; }}
        onMouseLeave={(e) => { if (node.body) e.currentTarget.style.color = "var(--text-soft)"; }}
      >
        <span>{label}</span>
        {node.filename && getFileIcon(node.filename)}
        {node.filename && (
          <span style={{ color: "var(--text)", fontWeight: "500" }}>{node.filename}</span>
        )}
        {addCount && <span style={{ color: "#34d399", fontWeight: "600", marginLeft: "4px" }}>{addCount}</span>}
        {delCount && <span style={{ color: "#f87171", fontWeight: "600", marginLeft: "2px" }}>{delCount}</span>}
        {node.body && (
          isExpanded ? <ChevronDown size={12} style={{ opacity: 0.8 }} /> : <ChevronRight size={12} style={{ opacity: 0.8 }} />
        )}
      </div>

      {isExpanded && node.body && (
        <div style={{
          marginTop: "6px",
          paddingLeft: "16px",
        }}>
          <pre style={{
            padding: "8px 12px",
            background: "color-mix(in oklch, var(--field), transparent 2%)",
            borderRadius: "6px",
            overflowX: "auto",
            maxHeight: "150px",
            whiteSpace: "pre-wrap",
            fontFamily: "var(--font-code)",
            fontSize: "11px",
            color: "var(--text-muted)",
            border: "1px solid var(--line-soft)",
            maxWidth: "600px"
          }}>
            {node.body}
          </pre>
        </div>
      )}
    </div>
  );
}

function normalizeProcessNoteText(text: string) {
  return text
    .replace(/\s+/g, " ")
    .replace(/\s+([,.;:!?，。；：！？])/g, "$1")
    .trim();
}

function splitProcessNotes(text: string) {
  return text
    .split(/\n{2,}|\n(?=(?:I will|I'll|Let me|Next|Now|我会|我先|接下来|现在|然后|下一步))/i)
    .map(normalizeProcessNoteText)
    .filter((line) => line && line !== "." && line !== "..." && line !== "…")
    .slice(0, 6);
}

function looksLikeProcessNarration(text: string) {
  const clean = normalizeProcessNoteText(text);
  if (!clean || clean.length > 520) return false;
  if (isRuntimeNoticeText(clean)) return false;
  if (/^#{1,6}\s|```|\|.+\||^\s*[-*]\s/m.test(text)) return false;
  if (/\b(the user|user hasn't|asked for|I've provided|wait for the user|user's response|want to dive deeper)\b/i.test(clean)) {
    return false;
  }
  return /^(I will|I'll|Let me|Next|Now|I need to|I’m going to|I'm going to|我会|我先|接下来|现在|然后|下一步|先)/i.test(clean) ||
    /(读取|查看|搜索|检查|运行|验证|修改|分析|探索).*(文件|项目|代码|目录|结构|实现|结果)/i.test(clean);
}

function isMostlyEnglishText(text: string) {
  const latin = (text.match(/[A-Za-z]/g) || []).length;
  const cjk = (text.match(/[\u4e00-\u9fff]/g) || []).length;
  return latin > 40 && cjk === 0;
}

function localizeProcessNoteText(text: string, appLang: string) {
  if (appLang !== "zh") return text;

  const clean = normalizeProcessNoteText(text);
  if (!clean || isRuntimeNoticeText(clean)) return "";
  if (/\b(the user|user hasn't|asked for|I've provided|wait for the user|user's response|want to dive deeper)\b/i.test(clean)) {
    return "";
  }
  if (/project structure/i.test(clean) && /source files/i.test(clean)) {
    return "我已经看到了项目结构，接下来继续查看关键源文件。";
  }
  if (/JS and CSS files|frontend/i.test(clean)) {
    return "我会继续检查 JS/CSS 等前端文件，补齐前端结构理解。";
  }
  if (/src subdirectories|key files/i.test(clean)) {
    return "我先查看 src 子目录和关键文件，理解项目结构。";
  }
  if (/source files/i.test(clean)) {
    return "我会继续查看源代码文件，理解项目实现。";
  }
  if (/comprehensive understanding/i.test(clean)) {
    return "我已经基本了解项目结构，接下来补充检查关键文件。";
  }

  const localized = clean
    .replace(/^I will read\b/i, "我会读取")
    .replace(/^I'll read\b/i, "我会读取")
    .replace(/^Let me read\b/i, "我先读取")
    .replace(/^Let me explore\b/i, "我先探索")
    .replace(/^Let me look at\b/i, "我先查看")
    .replace(/^Let me also check\b/i, "我再检查")
    .replace(/^I will search for\b/i, "我会搜索")
    .replace(/^I'll search for\b/i, "我会搜索")
    .replace(/^Let me search for\b/i, "我先搜索")
    .replace(/^I will inspect\b/i, "我会检查")
    .replace(/^I'll inspect\b/i, "我会检查")
    .replace(/^Let me inspect\b/i, "我先检查")
    .replace(/^I will run\b/i, "我会运行")
    .replace(/^I'll run\b/i, "我会运行")
    .replace(/^Let me run\b/i, "我先运行")
    .replace(/^I need to\b/i, "我需要")
    .replace(/^I’m going to\b/i, "我会")
    .replace(/^I'm going to\b/i, "我会")
    .replace(/^Next,\s*/i, "接下来，")
    .replace(/^Now,\s*/i, "现在，")
    .replace(/\bto see\b/i, "，确认")
    .replace(/\bto inspect\b/i, "，检查")
    .replace(/\bto understand\b/i, "，理解")
    .replace(/\bwhere it is defined\b/i, "它的定义位置")
    .replace(/\bhow\b/i, "如何");

  if (isMostlyEnglishText(localized)) {
    return "";
  }
  return localized;
}

function ProcessNoteNode({ note, appLang }: { note: Extract<TimelineItem, { kind: "process_note" }>; appLang: string }) {
  const isZh = appLang === "zh";
  const isRunning = note.status === "running";
  const title = note.title || (isRunning ? (isZh ? "正在思考" : "Thinking") : "");
  const body = localizeProcessNoteText(note.body, appLang);

  if (!body && !title) return null;

  return (
    <div
      style={{
        maxWidth: "760px",
        width: "100%",
        margin: "6px auto 10px",
        paddingLeft: "33px",
        color: "var(--text)",
      }}
    >
      {title && (
        <div
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "6px",
            marginBottom: body ? "8px" : 0,
            color: isRunning ? "var(--accent)" : "var(--text-soft)",
            fontSize: "12.5px",
            fontWeight: 560,
          }}
        >
          {isRunning ? (
            <CircleDot size={10} className="glowing-logo" />
          ) : null}
          <span>{title}</span>
          {!isRunning ? <ChevronRight size={12} style={{ opacity: 0.55 }} /> : null}
        </div>
      )}
      {body && (
        <div
          style={{
            maxWidth: "72ch",
            color: "var(--text)",
            fontSize: "14.5px",
            lineHeight: 1.58,
            fontWeight: 500,
          }}
        >
          {renderInlineMarkdown(body)}
        </div>
      )}
    </div>
  );
}

function hasVisibleProcessBody(item: TimelineItem | undefined) {
  return item?.kind === "process_note" && Boolean(item.body.trim());
}

function processNote(
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

function syntheticNarrationForActivity(item: TimelineItem, appLang: string) {
  const isZh = appLang === "zh";
  if (!isZh) return "";

  if (item.kind === "activity_group") {
    const visibleItems = summarizeActivityItems(item.items || []);
    const preview = activityGroupPreview(visibleItems, appLang);
    if (item.type === "explore") {
      return `我先查看 ${preview}，确认项目结构、关键文件和后续分析入口。`;
    }
    if (item.type === "search") {
      return `我先搜索 ${preview}，缩小需要继续查看的范围。`;
    }
    if (item.type === "run") {
      return `我会运行 ${preview}，用实际输出验证当前判断。`;
    }
    return `我会执行 ${preview}，把结果用于下一步判断。`;
  }

  if (item.kind === "activity_item") {
    return item.filename
      ? `我会修改 ${item.filename}，然后用构建或测试确认改动是否生效。`
      : "我会完成这处修改，然后继续验证效果。";
  }

  return "";
}

function syntheticNarrationBeforeAssistant(previous: TimelineItem | undefined, appLang: string) {
  if (appLang !== "zh") return "";
  if (!previous || (previous.kind !== "activity_group" && previous.kind !== "activity_item")) return "";
  if (previous.kind === "activity_group") {
    if (previous.type === "explore") {
      return "我已经完成基础探索，下面根据看到的结构和文件内容整理结论。";
    }
    if (previous.type === "run") {
      return "验证命令已经返回，下面结合结果给出结论。";
    }
  }
  if (previous.kind === "activity_item") {
    return "修改已经完成，下面总结改动和验证结果。";
  }
  return "";
}

function addSyntheticProcessNarration(items: TimelineItem[], appLang: string) {
  const next: TimelineItem[] = [];

  items.forEach((item) => {
    const previous = next[next.length - 1];

    if ((item.kind === "activity_group" || item.kind === "activity_item") && !hasVisibleProcessBody(previous)) {
      const body = syntheticNarrationForActivity(item, appLang);
      if (body) {
        next.push(processNote(`${item.id}-auto-process-before`, body));
      }
    }

    if (item.kind === "assistant" && !isIntermediateAssistantItem(item) && !hasVisibleProcessBody(previous)) {
      const body = syntheticNarrationBeforeAssistant(previous, appLang);
      if (body) {
        next.push(processNote(`${item.id}-auto-process-before`, body));
      }
    }

    next.push(item);
  });

  return next;
}

function compileInlineItems(items: TimelineItem[], isTurnActive?: boolean, appLang = "zh"): TimelineItem[] {
  const result: TimelineItem[] = [];
  let buffer: Array<Extract<TimelineItem, { kind: "tool" }>> = [];

  const getToolType = (toolName: string, title: string) => {
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
    
    let groupType: "explore" | "search" | "run" | "other" = "explore";
    const tools = buffer.filter(item => item.kind === "tool");
    const visibleTools = tools.filter(item => !shouldHideActivityItem(item));
    
    const runTools = visibleTools.filter(t => getToolType(t.tool || "", t.title || "") === "run");
    const searchTools = visibleTools.filter(t => getToolType(t.tool || "", t.title || "") === "search");
    const exploreTools = visibleTools.filter(t => getToolType(t.tool || "", t.title || "") === "explore");
    
    if (runTools.length > 0) {
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
      label = isRunning ? "Exploring" : "Explored";
      label += ` ${count} file${count > 1 ? "s" : ""}`;
    } else if (groupType === "search") {
      label = isRunning ? "Searching web" : "Searched web";
      label += ` ${count} time${count > 1 ? "s" : ""}`;
    } else if (groupType === "run") {
      label = isRunning ? "Running" : "Ran";
      label += ` ${count} command${count > 1 ? "s" : ""}`;
    } else {
      label = isRunning ? "Executing" : "Executed";
      label += ` ${count} action${count > 1 ? "s" : ""}`;
    }
    
    result.push({
      id: `group-${buffer[0].id}`,
      kind: "activity_group",
      type: groupType,
      status: isRunning ? "running" : "success",
      label,
      items: [...buffer]
    });
    
    buffer = [];
  };

  const pushProcessNotes = (item: TimelineItem, title?: string, status?: "running" | "success") => {
    const notes = splitProcessNotes((item as any).body || "");
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
    } else if (item.kind === "tool") {
      if (item.title === "工具结果") {
        let found = false;
        for (let i = buffer.length - 1; i >= 0; i--) {
          if (buffer[i].kind === "tool") {
            buffer[i].result = item.body;
            found = true;
            break;
          }
        }
        if (!found) {
          for (let i = result.length - 1; i >= 0; i--) {
            const resultItem = result[i];
            if (resultItem.kind === "activity_item" && resultItem.tool) {
              resultItem.result = item.body;
              found = true;
              break;
            } else if (resultItem.kind === "activity_group") {
              const groupItems = resultItem.items;
              for (let j = groupItems.length - 1; j >= 0; j--) {
                const groupItem = groupItems[j];
                if (groupItem.kind === "tool") {
                  groupItem.result = item.body;
                  found = true;
                  break;
                }
              }
              if (found) break;
            }
          }
        }
        continue;
      }

      const type = getToolType(item.tool || "", item.title || "");
      if (type === "edit") {
        flushBuffer();
        const parsed = parseToolDetails(item);
        result.push({
          id: item.id,
          kind: "activity_item",
          type: "edit",
          tool: item.tool,
          title: item.title,
          body: item.body,
          status: item.status,
          filename: parsed.filename,
          diff: parsed.diff
        });
      } else {
        buffer.push(item);
      }
    } else if (item.kind === "reasoning") {
      flushBuffer();
      if (item.meta === "running" && isTurnActive) {
        result.push({
          id: `${item.id}-process`,
          kind: "process_note",
          title: item.title || "正在思考",
          body: "",
          status: "running"
        });
      } else if (isThinkingStatusTitle(item.title)) {
        result.push({
          id: `${item.id}-process`,
          kind: "process_note",
          title: item.title,
          body: "",
          status: "success"
        });
      }
    }
  }
  flushBuffer();
  const enrichedResult = addSyntheticProcessNarration(result, appLang);

  let lastAssistantIdx = -1;
  for (let i = enrichedResult.length - 1; i >= 0; i--) {
    if (enrichedResult[i].kind === "assistant") {
      lastAssistantIdx = i;
      break;
    }
  }

  if (!isTurnActive && lastAssistantIdx !== -1) {
    return enrichedResult.filter((item, idx) => {
      if ((item.kind === "activity_group" || item.kind === "activity_item" || item.kind === "process_note") && idx > lastAssistantIdx) {
        return false;
      }
      return true;
    });
  }

  return enrichedResult;
}

export function App() {
  const [bootstrap, setBootstrap] = useState<Bootstrap>(fallbackBootstrap);
  const [viewMode, setViewMode] = useState<ViewMode>(() => {
    return (localStorage.getItem("yode-view-mode") as ViewMode) || "chat";
  });
  const [appLang, setAppLang] = useState(() => localStorage.getItem("yode-language") || "zh");
  const [draft, setDraft] = useState("");
  const [sessionItems, setSessionItems] = useState<SessionSummary[]>([]);
  const [timelineItems, setTimelineItems] = useState<TimelineItem[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [projectRoots, setProjectRoots] = useState<string[]>(() => loadStoredProjectRoots());
  const [projectOrder, setProjectOrder] = useState<string[]>(() => loadStoredProjectOrder());
  const [selectedProjectRoot, setSelectedProjectRoot] = useState<string | null | undefined>(() => loadStoredSelectedProjectRoot());
  const [inspectorOpen, setInspectorOpen] = useState(true);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [messageQueue, setMessageQueue] = useState<string[]>([]);
  const [currentTurnId, setCurrentTurnId] = useState<string | null>(null);
  const [permissionMode, setPermissionMode] = useState<string>("default");
  const [pendingUserQuestion, setPendingUserQuestion] = useState<PendingUserQuestion | null>(null);
  const activeSessionIdRef = useRef<string | null>(null);

  useEffect(() => {
    activeSessionIdRef.current = activeSessionId;
  }, [activeSessionId]);

  const handlePermissionModeChange = (mode: string) => {
    setPermissionMode(mode);
    setBootstrap(prev => ({ ...prev, permissionMode: mode }));
    invoke("permission_mode_set", { mode }).catch(console.error);
  };

  const handleUpdateProvider = async (provider: string) => {
    const saved = localStorage.getItem("yode-llm-providers");
    let models: string[] = [];
    if (saved) {
      try {
        const data = JSON.parse(saved);
        const list = Array.isArray(data) ? data : Object.values(data);
        const found = list.find((p: any) => p && p.id === provider);
        if (found && Array.isArray(found.models)) {
          models = found.models;
        }
      } catch (e) {}
    }
    if (models.length === 0) {
      const meta = PROVIDERS_META.find(p => p.id === provider);
      models = meta ? meta.defaultModels : [];
    }
    const lastModelKey = `yode-last-model-${provider}`;
    const lastUsedModel = localStorage.getItem(lastModelKey);
    const defaultModel = (lastUsedModel && models.includes(lastUsedModel)) ? lastUsedModel : (models[0] || "");

    if (activeSessionId) {
      setSessionItems((items) =>
        items.map((s) =>
          s.id === activeSessionId ? { ...s, provider, model: defaultModel } : s
        )
      );
      try {
        await invoke("sessions_update_llm", {
          sessionId: activeSessionId,
          provider,
          model: defaultModel
        });
      } catch (err) {
        console.error(err);
      }
    } else {
      setBootstrap((prev) => ({ ...prev, provider, model: defaultModel }));
    }
  };

  const handleUpdateModel = async (model: string) => {
    localStorage.setItem(`yode-last-model-${currentProvider}`, model);

    if (activeSessionId) {
      setSessionItems((items) =>
        items.map((s) =>
          s.id === activeSessionId ? { ...s, model } : s
        )
      );
      try {
        await invoke("sessions_update_llm", {
          sessionId: activeSessionId,
          provider: currentProvider,
          model
        });
      } catch (err) {
        console.error(err);
      }
    } else {
      setBootstrap((prev) => ({ ...prev, model }));
    }
  };

  useEffect(() => {
    const handleLangChange = (e: Event) => {
      const newLang = (e as CustomEvent).detail;
      setAppLang(newLang);
    };
    window.addEventListener("yode-language-change", handleLangChange);
    return () => window.removeEventListener("yode-language-change", handleLangChange);
  }, []);

  useEffect(() => {
    localStorage.setItem(PROJECT_ROOTS_STORAGE_KEY, JSON.stringify(projectRoots));
  }, [projectRoots]);

  useEffect(() => {
    localStorage.setItem(PROJECT_ORDER_STORAGE_KEY, JSON.stringify(projectOrder));
  }, [projectOrder]);

  useEffect(() => {
    if (selectedProjectRoot === undefined) return;
    localStorage.setItem(
      SELECTED_PROJECT_ROOT_STORAGE_KEY,
      selectedProjectRoot === null ? STANDALONE_PROJECT_SENTINEL : selectedProjectRoot
    );
  }, [selectedProjectRoot]);

  // Load theme & settings on startup to avoid styling flashes
  useEffect(() => {
    const root = document.documentElement;

    // Mode
    const themeMode = localStorage.getItem("yode-theme-mode") || "dark";
    root.classList.remove("light", "dark");
    if (themeMode === "light") {
      root.classList.add("light");
      root.style.setProperty("color-scheme", "light");
    } else if (themeMode === "dark") {
      root.classList.add("dark");
      root.style.setProperty("color-scheme", "dark");
    } else {
      const isSystemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
      root.classList.add(isSystemDark ? "dark" : "light");
      root.style.setProperty("color-scheme", isSystemDark ? "dark" : "light");
    }

    // Colors & Fonts
    const accentColor = localStorage.getItem("yode-accent-color") || "#FF79C6";
    const backgroundColor = localStorage.getItem("yode-bg-color") || "#282A36";
    const foregroundColor = localStorage.getItem("yode-fg-color") || "#F8F8F2";
    const uiFont = localStorage.getItem("yode-ui-font") || "-apple-system, BlinkMacSystemFont, \"Segoe UI\", system-ui, sans-serif";
    const codeFont = localStorage.getItem("yode-code-font") || "ui-monospace, \"SF Mono\", SFMono-Regular, Menlo, Monaco, Consolas, monospace";
    const codeFontSize = localStorage.getItem("yode-code-font-size") || "12";
    const contrast = localStorage.getItem("yode-contrast") || "48";
    const uiFontSize = localStorage.getItem("yode-ui-font-size") || "13";

    root.style.setProperty("--accent", accentColor);
    root.style.setProperty("--bg", backgroundColor);
    root.style.setProperty("--text", foregroundColor);
    root.style.setProperty("--font-ui", uiFont);
    root.style.setProperty("--font-code", codeFont);
    root.style.setProperty("--code-font-size", `${codeFontSize}px`);
    root.style.setProperty("--contrast-val", contrast);
    root.style.fontSize = `${uiFontSize}px`;

    // Deriving colors based on background color lightness
    const hexToRgb = (hex: string) => {
      const shorthandRegex = /^#?([a-f\d])([a-f\d])([a-f\d])$/i;
      const fullHex = hex.replace(shorthandRegex, (_, r, g, b) => r + r + g + g + b + b);
      const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(fullHex);
      return result ? {
        r: parseInt(result[1], 16),
        g: parseInt(result[2], 16),
        b: parseInt(result[3], 16)
      } : null;
    };
    const rgbToHex = (r: number, g: number, b: number) => {
      const toHex = (c: number) => {
        const hex = Math.max(0, Math.min(255, c)).toString(16);
        return hex.length === 1 ? "0" + hex : hex;
      };
      return "#" + toHex(r) + toHex(g) + toHex(b);
    };
    const isLightColor = (hex: string) => {
      const rgb = hexToRgb(hex);
      if (!rgb) return false;
      const luminance = 0.299 * rgb.r + 0.587 * rgb.g + 0.114 * rgb.b;
      return luminance > 128;
    };
    const adjustBrightness = (hex: string, percent: number) => {
      const rgb = hexToRgb(hex);
      if (!rgb) return hex;
      const factor = 1 + (percent / 100);
      const r = Math.max(0, Math.min(255, Math.round(rgb.r * factor)));
      const g = Math.max(0, Math.min(255, Math.round(rgb.g * factor)));
      const b = Math.max(0, Math.min(255, Math.round(rgb.b * factor)));
      return rgbToHex(r, g, b);
    };

    const light = isLightColor(backgroundColor);
    const bgPercentMod = light ? -5 : 5;
    const bgDoubleMod = light ? -10 : 10;
    const bgTripleMod = light ? -15 : 15;
    const borderMod = light ? -18 : 18;
    const borderSoftMod = light ? -10 : 10;

    const chromeColor = adjustBrightness(backgroundColor, bgPercentMod);
    const panelColor = adjustBrightness(backgroundColor, bgDoubleMod);
    const panelRaised = adjustBrightness(backgroundColor, bgTripleMod);
    const fieldColor = adjustBrightness(backgroundColor, bgPercentMod);
    const lineColor = adjustBrightness(backgroundColor, borderMod);
    const lineSoftColor = adjustBrightness(backgroundColor, borderSoftMod);

    const rgbAccent = hexToRgb(accentColor);
    const accentMuted = rgbAccent ? `rgba(${rgbAccent.r}, ${rgbAccent.g}, ${rgbAccent.b}, 0.2)` : "rgba(255, 255, 255, 0.1)";

    root.style.setProperty("--chrome", chromeColor);
    root.style.setProperty("--panel", panelColor);
    root.style.setProperty("--panel-raised", panelRaised);
    root.style.setProperty("--field", fieldColor);
    root.style.setProperty("--line", lineColor);
    root.style.setProperty("--line-soft", lineSoftColor);
    root.style.setProperty("--accent-muted", accentMuted);

    // Pointer cursors
    if (localStorage.getItem("yode-use-pointers") === "true") {
      document.body.classList.add("use-pointers");
    }

    // Reduce Motion
    const reduceMotion = localStorage.getItem("yode-reduce-motion") || "system";
    if (reduceMotion === "on") {
      document.body.classList.add("reduce-motion");
    } else if (reduceMotion === "system") {
      const prefersReduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      if (prefersReduced) {
        document.body.classList.add("reduce-motion");
      }
    }

    // Font Smoothing
    const fontSmoothing = localStorage.getItem("yode-font-smoothing");
    if (fontSmoothing === null || fontSmoothing === "true") {
      document.body.classList.add("font-smoothing");
    } else {
      document.body.classList.add("no-font-smoothing");
    }
  }, []);

  useEffect(() => {
    // Sync translucent class to app-shell based on saved value
    const val = localStorage.getItem("yode-translucent-sidebar");
    const isTranslucent = val === null ? true : val === "true";
    const shells = document.querySelectorAll(".app-shell");
    shells.forEach(shell => {
      if (isTranslucent) {
        shell.classList.add("translucent-sidebar");
        shell.classList.remove("translucent-sidebar-disabled");
      } else {
        shell.classList.remove("translucent-sidebar");
        shell.classList.add("translucent-sidebar-disabled");
      }
    });
  }, [viewMode]);

  const loadBootstrap = () => {
    invoke<Bootstrap>("app_get_bootstrap")
      .then((nextBootstrap) => {
        setBootstrap(nextBootstrap);
        setPermissionMode(nextBootstrap.permissionMode);
        setSelectedProjectRoot((current) =>
          current === undefined || current === fallbackBootstrap.workspacePath
            ? nextBootstrap.workspacePath
            : current
        );

        const activeSessions = visibleSessions(nextBootstrap.sessions);
        const activeSessionId = activeSessions.find((session) => session.active)?.id ?? null;

        setSessionItems(activeSessions);
        setProjectRoots((current) =>
          dedupeProjectRoots([
            ...current,
            ...activeSessions.map((session) => session.projectRoot),
          ])
        );
        activeSessionIdRef.current = activeSessionId;
        setActiveSessionId(activeSessionId);
        if (activeSessionId && "__TAURI_INTERNALS__" in window) {
          invoke<DesktopMessage[]>("sessions_messages", {
            sessionId: activeSessionId,
            session_id: activeSessionId
          })
            .then((messages) => {
              if (activeSessionIdRef.current === activeSessionId) {
                setTimelineItems(messagesToTimelineItems(messages));
              }
            })
            .catch(console.error);
        }
      })
      .catch(() => {
        setBootstrap(fallbackBootstrap);
        if (!("__TAURI_INTERNALS__" in window)) {
          const activeSessions = visibleSessions(sessions);
          const activeSessionId = activeSessions.find((session) => session.active)?.id ?? null;

          setSessionItems(activeSessions);
          activeSessionIdRef.current = activeSessionId;
          setActiveSessionId(activeSessionId);
          setSelectedProjectRoot((current) =>
            current === undefined ? fallbackBootstrap.workspacePath : current
          );
          setTimelineItems(timeline);
        }
      });
  };

  useEffect(() => {
    loadBootstrap();
  }, []);

  useEffect(() => {
    const handleUnarchive = () => {
      loadBootstrap();
    };
    const handlePermanentDelete = (event: Event) => {
      const sessionId = (event as CustomEvent<{ sessionId?: string }>).detail?.sessionId;
      if (!sessionId) {
        loadBootstrap();
        return;
      }
      setSessionItems((items) => items.filter((session) => session.id !== sessionId));
      setActiveSessionId((current) => current === sessionId ? null : current);
    };
    window.addEventListener("yode-session-unarchived", handleUnarchive);
    window.addEventListener("yode-session-deleted-permanently", handlePermanentDelete);
    return () => {
      window.removeEventListener("yode-session-unarchived", handleUnarchive);
      window.removeEventListener("yode-session-deleted-permanently", handlePermanentDelete);
    };
  }, []);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) {
      return;
    }

    let active = true;
    let disposeFn: (() => void) | undefined;

    listen<DesktopEvent>("desktop-event", (event) => {
      if (!active) return;
      const payload = event.payload;
      const outer = (payload as any).kind ? (payload as DesktopEvent) : null;
      const kind = outer ? outer.kind : (event as any).kind;
      const eventSessionId = outer?.sessionId ?? (payload as any).sessionId;
      if (
        eventSessionId &&
        activeSessionIdRef.current &&
        eventSessionId !== activeSessionIdRef.current
      ) {
        return;
      }

      if (kind === "turn_started") {
        setIsProcessing(true);
        if (outer) {
          setCurrentTurnId(outer.turnId);
        }
      } else if (kind === "ask_user" && eventSessionId && (outer?.turnId ?? (payload as any).turnId)) {
        setPendingUserQuestion({
          sessionId: eventSessionId,
          turnId: outer?.turnId ?? (payload as any).turnId,
          question: String((payload as any).payload?.body ?? "请回复问题")
        });
      } else if (kind === "turn_completed" || kind === "error") {
        setIsProcessing(false);
        setPendingUserQuestion(null);
      }

      setTimelineItems((items) =>
        applyDesktopEventToTimelineItems(
          items,
          (payload as any).kind ? (payload as DesktopEvent) : payload,
          (payload as any).kind ? undefined : (event as any).kind
        )
      );
    })
      .then((dispose) => {
        if (!active) {
          dispose();
        } else {
          disposeFn = dispose;
        }
      })
      .catch(console.error);

    return () => {
      active = false;
      if (disposeFn) {
        disposeFn();
      }
    };
  }, []);

  const activeSession = useMemo(
    () =>
      activeSessionId
        ? sessionItems.find((session) => session.id === activeSessionId) ?? null
        : null,
    [activeSessionId, sessionItems]
  );
  const currentProvider = activeSession?.provider ?? bootstrap.provider;
  const currentModel = activeSession?.model ?? bootstrap.model;

  const projectOptions = useMemo(() => {
    const roots = dedupeProjectRoots([
      bootstrap.workspacePath,
      ...projectRoots,
      ...sessionItems.map((session) => session.projectRoot),
    ]);
    return [
      ...roots.map((root) => ({
        label: projectLabelFromPath(root),
        root,
      })),
      { label: "独立对话", root: null }
    ];
  }, [bootstrap.workspacePath, projectRoots, sessionItems]);

  useEffect(() => {
    const roots = projectOptions
      .map((option) => option.root)
      .filter((root): root is string => Boolean(root));
    setProjectOrder((current) => [
      ...current.filter((root) => roots.includes(root)),
      ...roots.filter((root) => !current.includes(root)),
    ]);
  }, [projectOptions]);

  const orderedProjectOptions = useMemo(() => {
    const orderIndex = new Map(projectOrder.map((root, index) => [root, index]));
    return [...projectOptions].sort((a, b) => {
      if (!a.root || !b.root) {
        return a.root ? -1 : b.root ? 1 : 0;
      }
      return (orderIndex.get(a.root) ?? Number.MAX_SAFE_INTEGER) -
        (orderIndex.get(b.root) ?? Number.MAX_SAFE_INTEGER);
    });
  }, [projectOptions, projectOrder]);

  const handleProjectReorder = (draggedRoot: string, targetRoot: string, placement: "before" | "after" = "before") => {
    if (draggedRoot === targetRoot) return;
    setProjectOrder((current) => {
      const roots = projectOptions
        .map((option) => option.root)
        .filter((root): root is string => Boolean(root));
      const base = [
        ...current.filter((root) => roots.includes(root)),
        ...roots.filter((root) => !current.includes(root)),
      ];
      const from = base.indexOf(draggedRoot);
      const targetIndex = base.indexOf(targetRoot);
      if (from < 0 || targetIndex < 0) return base;
      const withoutDragged = base.filter((root) => root !== draggedRoot);
      const to = withoutDragged.indexOf(targetRoot);
      if (from < 0 || to < 0) return base;
      const insertIndex = placement === "after" ? to + 1 : to;
      const next = [...withoutDragged];
      next.splice(insertIndex, 0, draggedRoot);
      return next;
    });
  };

  function handleCreateSession(projectRoot?: string | null) {
    setActiveSessionId(null);
    setCurrentTurnId(null);
    setMessageQueue([]);
    setIsProcessing(false);
    setPendingUserQuestion(null);
    setSessionItems((items) => items.map((item) => ({ ...item, active: false })));
    setTimelineItems([]);
    if (projectRoot !== undefined) {
      setSelectedProjectRoot(projectRoot);
    }
  }

  async function handleAddProject() {
    const pickedRoot = await invoke<string | null>("project_folder_pick").catch((err) => {
      console.error(err);
      return null;
    });
    const normalized = normalizeProjectRoot(pickedRoot);
    if (!normalized) return;
    setProjectRoots((current) => dedupeProjectRoots([...current, normalized]));
    setSelectedProjectRoot(normalized);
  }

  async function handleSendMessage() {
    if (!draft.trim()) return;
    const content = draft.trim();

    if (pendingUserQuestion) {
      setDraft("");
      setTimelineItems((items) => [
        ...items,
        {
          id: `ask-answer-${Date.now()}`,
          kind: "user",
          title: "用户",
          body: content,
          createdAt: Date.now()
        }
      ]);
      await invoke("ask_user_respond", {
        sessionId: pendingUserQuestion.sessionId,
        session_id: pendingUserQuestion.sessionId,
        turnId: pendingUserQuestion.turnId,
        turn_id: pendingUserQuestion.turnId,
        answer: content
      }).catch((err) => {
        console.error(err);
        setTimelineItems((items) => [
          ...items,
          {
            id: `ask-answer-error-${Date.now()}`,
            kind: "assistant",
            title: "错误",
            body: "发送问题回复失败。",
            meta: "stream complete"
          }
        ]);
      });
      setPendingUserQuestion(null);
      return;
    }

    const sessionIdAtSend = activeSession?.id ?? null;
    const projectRootAtSend = selectedProjectRoot === undefined ? bootstrap.workspacePath : selectedProjectRoot;
    setDraft("");

    if (isProcessing) {
      setMessageQueue((prev) => [...prev, content]);
      setTimelineItems((items) => [
        ...items,
        {
          id: `local-queued-${Date.now()}`,
          kind: "user",
          title: "用户 (等待中...)",
          body: content,
          createdAt: Date.now()
        }
      ]);
      return;
    }

    setIsProcessing(true);
    setTimelineItems((items) => [
      ...items,
      {
        id: `local-${Date.now()}`,
        kind: "user",
        title: "用户",
        body: content,
        createdAt: Date.now()
      }
    ]);

    try {
      const res = await invoke<TurnAccepted>("turn_send_message", {
        request: {
          sessionId: sessionIdAtSend,
          content,
          projectRoot: sessionIdAtSend ? undefined : projectRootAtSend,
          standalone: sessionIdAtSend ? undefined : projectRootAtSend === null,
          title: sessionIdAtSend ? undefined : deriveSessionTitle(content),
          provider: currentProvider,
          model: currentModel
        }
      });
      setCurrentTurnId(res.turnId);
      activeSessionIdRef.current = res.sessionId;
      setActiveSessionId(res.sessionId);
      setSessionItems((items) => upsertActiveSession(items, res.session));
    } catch (err) {
      console.error(err);
      setIsProcessing(false);
      setDraft(content);
    }
  }

  useEffect(() => {
    if (!isProcessing && messageQueue.length > 0 && activeSession?.id) {
      const nextContent = messageQueue[0];
      setMessageQueue((prev) => prev.slice(1));
      setIsProcessing(true);
      
      setTimelineItems((items) =>
        items.map((item) =>
          item.kind === "user" && item.body === nextContent && item.title.includes("等待中")
            ? { ...item, title: "用户" }
            : item
        )
      );

      invoke<TurnAccepted>("turn_send_message", {
        request: {
          sessionId: activeSession.id,
          content: nextContent,
          projectRoot: undefined,
          standalone: undefined,
          title: undefined,
          provider: undefined,
          model: undefined
        }
      }).then((res) => {
        setCurrentTurnId(res.turnId);
        activeSessionIdRef.current = res.sessionId;
        setSessionItems((items) => upsertActiveSession(items, res.session));
      }).catch((err) => {
        console.error(err);
        setIsProcessing(false);
      });
    }
  }, [isProcessing, messageQueue, activeSession?.id]);

  async function handleCancelMessage() {
    if (activeSession?.id && currentTurnId) {
      await invoke("turn_cancel", {
        sessionId: activeSession.id,
        turnId: currentTurnId
      }).catch(console.error);
      setIsProcessing(false);
    }
  }

  async function handleSelectSession(sessionId: string) {
    const nextSession = sessionItems.find((item) => item.id === sessionId);
    activeSessionIdRef.current = sessionId;
    setActiveSessionId(sessionId);
    setSelectedProjectRoot(nextSession?.projectRoot ?? null);
    setIsProcessing(false);
    setCurrentTurnId(null);
    setMessageQueue([]);
    setPendingUserQuestion(null);

    if (!("__TAURI_INTERNALS__" in window)) {
      setTimelineItems(timeline);
      return;
    }

    try {
      const messages = await invoke<DesktopMessage[]>("sessions_messages", {
        sessionId,
        session_id: sessionId
      });
      if (activeSessionIdRef.current !== sessionId) return;
      setTimelineItems(messagesToTimelineItems(messages));
    } catch (err) {
      if (activeSessionIdRef.current !== sessionId) return;
      console.error(err);
      setTimelineItems([
        {
          id: `history-error-${Date.now()}`,
          kind: "assistant",
          title: "错误",
          body: "加载历史对话失败。",
          meta: "stream complete"
        }
      ]);
    }
  }

  const handleSetViewMode = (mode: ViewMode) => {
    setViewMode(mode);
    localStorage.setItem("yode-view-mode", mode);
  };

  if (viewMode === "settings") {
    return (
      <main className="app-shell" style={{ display: "block", width: "100vw", height: "100vh", overflow: "hidden" }}>
        <SettingsShell bootstrap={bootstrap} onClose={() => handleSetViewMode("chat")} />
      </main>
    );
  }

  const handleDeleteSession = (sessionId: string) => {
    const session = sessionItems.find(s => s.id === sessionId);
    if (!session) return;

    // 1. Get and update yode-archived-session-ids
    const savedIds = localStorage.getItem(ARCHIVED_SESSION_IDS_STORAGE_KEY);
    let archivedIds: string[] = [];
    if (savedIds) {
      try {
        archivedIds = JSON.parse(savedIds);
      } catch (e) {}
    }
    if (!archivedIds.includes(sessionId)) {
      archivedIds.push(sessionId);
    }
    localStorage.setItem(ARCHIVED_SESSION_IDS_STORAGE_KEY, JSON.stringify(archivedIds));

    // 2. Get and update yode-archived-chats
    const savedChats = localStorage.getItem("yode-archived-chats");
    let archivedChats: any[] = [];
    if (savedChats) {
      try {
        archivedChats = JSON.parse(savedChats);
      } catch (e) {}
    }
    if (!archivedChats.some(c => c.id === sessionId)) {
      archivedChats.push({
        id: sessionId,
        title: session.title,
        date: session.updatedAt,
        project: session.project || "default"
      });
    }
    localStorage.setItem("yode-archived-chats", JSON.stringify(archivedChats));

    // 3. Filter state
    setSessionItems(prev => prev.filter(s => s.id !== sessionId));
  };

  const isStandalone = activeSession
    ? !activeSession.projectRoot
    : selectedProjectRoot === null;

  const displayedWorkspacePath = isStandalone
    ? null
    : (activeSession?.projectRoot ?? selectedProjectRoot ?? bootstrap.workspacePath);

  return (
    <main className="app-shell">
      <Sidebar
        sessions={sessionItems}
        projectOptions={orderedProjectOptions}
        activeSessionId={activeSessionId}
        viewMode={viewMode}
        onChangeView={handleSetViewMode}
        onCreateSession={handleCreateSession}
        onSelectSession={(sessionId) => {
          void handleSelectSession(sessionId);
        }}
        onAddProject={handleAddProject}
        onProjectReorder={handleProjectReorder}
        onDeleteSession={handleDeleteSession}
      />
      <section className="workspace" style={{ position: "relative", overflow: "hidden" }}>
        <Topbar
          bootstrap={bootstrap}
          sessionTitle={activeSession?.title ?? (appLang === "zh" ? "新对话" : "New chat")}
          workspacePath={displayedWorkspacePath}
          inspectorOpen={inspectorOpen}
          isProcessing={isProcessing && !pendingUserQuestion}
          onToggleInspector={() => setInspectorOpen(!inspectorOpen)}
          terminalOpen={terminalOpen}
          onToggleTerminal={() => setTerminalOpen(!terminalOpen)}
          currentProvider={currentProvider}
          currentModel={currentModel}
          onProviderChange={handleUpdateProvider}
          onModelChange={handleUpdateModel}
        />
        <ChatWorkspace
          draft={draft}
          timelineItems={timelineItems}
          onDraftChange={setDraft}
          onSendMessage={handleSendMessage}
          inspectorOpen={inspectorOpen}
          isProcessing={isProcessing}
          onCancelMessage={handleCancelMessage}
          permissionMode={permissionMode}
          onPermissionModeChange={handlePermissionModeChange}
          onPermissionResolved={(id) => {
            setTimelineItems((items) => items.filter((item) => item.id !== id));
          }}
          appLang={appLang}
          projectOptions={orderedProjectOptions}
          selectedProjectRoot={selectedProjectRoot === undefined ? bootstrap.workspacePath : selectedProjectRoot}
          onProjectRootChange={setSelectedProjectRoot}
          onAddProject={handleAddProject}
          currentProvider={currentProvider}
          currentModel={currentModel}
          onModelChange={handleUpdateModel}
        />
        <TerminalDrawer isOpen={terminalOpen} onClose={() => setTerminalOpen(false)} />
      </section>
    </main>
  );
}
function Sidebar({
  sessions,
  projectOptions,
  activeSessionId,
  viewMode,
  onChangeView,
  onCreateSession,
  onSelectSession,
  onAddProject,
  onProjectReorder,
  onDeleteSession
}: {
  sessions: SessionSummary[];
  projectOptions: Array<{ label: string; root: string | null }>;
  activeSessionId: string | null;
  viewMode: ViewMode;
  onChangeView: (mode: ViewMode) => void;
  onCreateSession: (projectRoot?: string | null) => void;
  onSelectSession: (sessionId: string) => void;
  onAddProject: () => Promise<void>;
  onProjectReorder: (draggedRoot: string, targetRoot: string, placement?: "before" | "after") => void;
  onDeleteSession: (sessionId: string) => void;
}) {
  const lang = localStorage.getItem("yode-language") || "zh";
  const isZh = lang === "zh";
  const t = (zhText: string, enText: string) => isZh ? zhText : enText;

  const [pinnedSessionIds, setPinnedSessionIds] = useState<string[]>(["s-1"]);
  const [deletingSessionId, setDeletingSessionId] = useState<string | null>(null);
  const [expandedProjectIds, setExpandedProjectIds] = useState<string[]>([]);
  const [draggingProjectId, setDraggingProjectId] = useState<string | null>(null);
  const [dragGhost, setDragGhost] = useState<{
    name: string;
    count: number;
    sessions: SessionSummary[];
    expanded: boolean;
    left: number;
    width: number;
    height: number;
    y: number;
  } | null>(null);
  
  // Hover information popover state
  const [hoveredSessionId, setHoveredSessionId] = useState<string | null>(null);
  const [hoverPosition, setHoverPosition] = useState<{ top: number; left: number } | null>(null);
  const hoverTimerRef = useRef<number | null>(null);
  const projectGroupsRef = useRef<Array<{ id: string; name: string; sessions: SessionSummary[] }>>([]);
  const projectNodeRefs = useRef(new Map<string, HTMLDivElement>());
  const projectFlipRectsRef = useRef(new Map<string, DOMRect>());
  const knownProjectIdsRef = useRef(new Set<string>());
  const dragStateRef = useRef<{
    id: string;
    name: string;
    count: number;
    sessions: SessionSummary[];
    expanded: boolean;
    left: number;
    width: number;
    height: number;
    offsetY: number;
    startY: number;
    hasMoved: boolean;
  } | null>(null);
  const suppressProjectClickRef = useRef(false);

  const handleMouseEnter = (sessionId: string, e: React.MouseEvent) => {
    // Clear any active timer
    if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
    
    const rect = e.currentTarget.getBoundingClientRect();
    const pos = {
      top: rect.top,
      left: 240
    };

    hoverTimerRef.current = window.setTimeout(() => {
      setHoveredSessionId(sessionId);
      setHoverPosition(pos);
    }, 600);
  };

  const handleMouseLeave = () => {
    if (hoverTimerRef.current) {
      window.clearTimeout(hoverTimerRef.current);
      hoverTimerRef.current = null;
    }
    setHoveredSessionId(null);
    setHoverPosition(null);
  };

  useEffect(() => {
    return () => {
      if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
    };
  }, []);

  const handleTogglePin = (sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setPinnedSessionIds(prev => 
      prev.includes(sessionId) 
        ? prev.filter(id => id !== sessionId) 
        : [...prev, sessionId]
    );
  };

  const handleDeleteClick = (sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setDeletingSessionId(sessionId);
  };

  const handleConfirmDelete = (sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    // execute delete
    onDeleteSession(sessionId);
    setDeletingSessionId(null);
  };

  const handleSessionMouseLeave = (sessionId: string) => {
    // If the mouse leaves the session button, cancel deletion mode
    if (deletingSessionId === sessionId) {
      setDeletingSessionId(null);
    }
    handleMouseLeave();
  };

  const { projectGroups, standaloneSessions } = useMemo(() => {
    const groupMap = new Map<string, SessionSummary[]>();
    const standalone: SessionSummary[] = [];

    sessions.forEach((session) => {
      const projectRoot = session.projectRoot?.trim();
      if (!projectRoot) {
        standalone.push(session);
        return;
      }
      const existing = groupMap.get(projectRoot) ?? [];
      existing.push(session);
      groupMap.set(projectRoot, existing);
    });

    const sortSessions = (items: SessionSummary[]) =>
      [...items].sort((a, b) => {
        const pinDelta = Number(pinnedSessionIds.includes(b.id)) - Number(pinnedSessionIds.includes(a.id));
        return pinDelta || 0;
      });

    return {
      projectGroups: projectOptions
        .filter((option) => option.root)
        .map((option) => ({
          id: option.root!,
          name: option.label,
          sessions: sortSessions(groupMap.get(option.root!) ?? [])
        })),
      standaloneSessions: sortSessions(standalone)
    };
  }, [pinnedSessionIds, projectOptions, sessions]);

  projectGroupsRef.current = projectGroups;
  const projectLayoutKey = useMemo(
    () => projectGroups.map((group) => group.id).join("\n"),
    [projectGroups]
  );

  useLayoutEffect(() => {
    const previousRects = projectFlipRectsRef.current;
    const nextRects = new Map<string, DOMRect>();

    projectGroupsRef.current.forEach((group) => {
      const node = projectNodeRefs.current.get(group.id);
      if (!node) return;
      const nextRect = node.getBoundingClientRect();
      nextRects.set(group.id, nextRect);
      if (group.id === draggingProjectId) return;
      const previousRect = previousRects.get(group.id);
      if (!previousRect) return;
      const deltaY = previousRect.top - nextRect.top;
      if (Math.abs(deltaY) < 0.5) return;
      if (document.body.classList.contains("reduce-motion")) return;
      node.animate(
        [
          { transform: `translateY(${deltaY}px)` },
          { transform: "translateY(0)" }
        ],
        {
          duration: 260,
          easing: "cubic-bezier(0.16, 1, 0.3, 1)"
        }
      );
    });

    projectFlipRectsRef.current = nextRects;
  }, [projectLayoutKey, draggingProjectId]);

  useEffect(() => {
    const currentProjectGroups = projectGroupsRef.current;
    const nextKnownProjectIds = new Set(currentProjectGroups.map((group) => group.id));
    const newlyDiscoveredProjectIds = currentProjectGroups
      .filter((group) => !knownProjectIdsRef.current.has(group.id))
      .map((group) => group.id);
    knownProjectIdsRef.current = nextKnownProjectIds;

    setExpandedProjectIds((current) => {
      const kept = current.filter((id) => nextKnownProjectIds.has(id));
      const next = [
        ...kept,
        ...newlyDiscoveredProjectIds.filter((id) => !kept.includes(id))
      ];
      return next;
    });
  }, [projectLayoutKey]);

  // Helper render method for a session item
  const renderSessionItem = (session: SessionSummary) => {
    const isPinned = pinnedSessionIds.includes(session.id);
    const isDeleting = deletingSessionId === session.id;
    const isActive = session.id === activeSessionId;

    return (
      <div
        className={`session-item-wrapper ${isActive ? "active" : ""}`}
        key={session.id}
        onMouseEnter={(e) => handleMouseEnter(session.id, e)}
        onMouseLeave={() => handleSessionMouseLeave(session.id)}
        style={{ position: "relative" }}
      >
        <button
          className={`session-button ${isActive ? "active" : ""}`}
          onClick={() => onSelectSession(session.id)}
          type="button"
        >
          <span className="session-title">
            {session.title}
          </span>
          {!isDeleting && (
            <span className="session-time" style={{ fontSize: "10.5px", color: "var(--text-soft)", marginLeft: "4px" }}>
              {session.updatedAt}
            </span>
          )}
        </button>

        {isDeleting ? (
          <div className="delete-confirm-overlay">
            <button
              onClick={(e) => handleConfirmDelete(session.id, e)}
              type="button"
              className="confirm-delete-btn"
            >
              {t("确认", "Confirm")}
            </button>
          </div>
        ) : (
          <div className="session-actions-overlay">
            <button
              onClick={(e) => handleTogglePin(session.id, e)}
              type="button"
              className="action-icon-btn"
              title={isPinned ? t("取消置顶", "Unpin") : t("置顶", "Pin")}
            >
              <Pin size={13} style={{ transform: isPinned ? "rotate(45deg)" : "none" }} />
            </button>
            <button
              onClick={(e) => handleDeleteClick(session.id, e)}
              type="button"
              className="action-icon-btn"
              title={t("删除", "Delete")}
            >
              <Trash2 size={13} />
            </button>
          </div>
        )}
      </div>
    );
  };

  const beginProjectPointerTracking = (
    group: { id: string; name: string; sessions: SessionSummary[] },
    event: React.PointerEvent<HTMLButtonElement>
  ) => {
    if (event.button !== 0) return;
    const groupNode = projectNodeRefs.current.get(group.id);
    const rect = (groupNode ?? event.currentTarget).getBoundingClientRect();
    const isExpandedAtStart = expandedProjectIds.includes(group.id);
    dragStateRef.current = {
      id: group.id,
      name: group.name,
      count: group.sessions.length,
      sessions: group.sessions,
      expanded: isExpandedAtStart,
      left: rect.left,
      width: rect.width,
      height: rect.height,
      offsetY: event.clientY - rect.top,
      startY: event.clientY,
      hasMoved: false
    };

    const handlePointerMove = (moveEvent: PointerEvent) => {
      const dragState = dragStateRef.current;
      if (!dragState) return;
      const moved = Math.abs(moveEvent.clientY - dragState.startY) > 4;
      if (!dragState.hasMoved) {
        if (!moved) return;
        dragState.hasMoved = true;
        suppressProjectClickRef.current = true;
        setDraggingProjectId(dragState.id);
        setDragGhost({
          name: dragState.name,
          count: dragState.count,
          sessions: dragState.sessions,
          expanded: dragState.expanded,
          left: dragState.left,
          width: dragState.width,
          height: dragState.height,
          y: moveEvent.clientY - dragState.offsetY
        });
      }

      moveEvent.preventDefault();
      setDragGhost((current) =>
        current ? { ...current, y: moveEvent.clientY - dragState.offsetY } : current
      );

      const groups = projectGroupsRef.current.filter((item) => item.id !== dragState.id);
      if (groups.length === 0) return;

      let targetId = groups[groups.length - 1].id;
      let placement: "before" | "after" = "after";
      for (const item of groups) {
        const node = projectNodeRefs.current.get(item.id);
        if (!node) continue;
        const itemRect = node.getBoundingClientRect();
        if (moveEvent.clientY < itemRect.top + itemRect.height / 2) {
          targetId = item.id;
          placement = "before";
          break;
        }
      }
      onProjectReorder(dragState.id, targetId, placement);
    };

    const finishPointerTracking = () => {
      dragStateRef.current = null;
      setDraggingProjectId(null);
      setDragGhost(null);
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", finishPointerTracking);
      window.removeEventListener("pointercancel", finishPointerTracking);
      window.setTimeout(() => {
        suppressProjectClickRef.current = false;
      }, 0);
    };

    window.addEventListener("pointermove", handlePointerMove, { passive: false });
    window.addEventListener("pointerup", finishPointerTracking);
    window.addEventListener("pointercancel", finishPointerTracking);
  };

  const renderProjectGroup = (group: { id: string; name: string; sessions: SessionSummary[] }) => {
    const expanded = expandedProjectIds.includes(group.id);
    const hasActiveSession = group.sessions.some((session) => session.id === activeSessionId);
    const isDragging = draggingProjectId === group.id;

    const style: React.CSSProperties = {
      position: "relative",
      zIndex: isDragging ? 10 : 1
    };

    return (
      <div
        className={`project-group ${hasActiveSession ? "active" : ""} ${isDragging ? "dragging" : ""}`}
        key={group.id}
        ref={(node) => {
          if (node) {
            projectNodeRefs.current.set(group.id, node);
          } else {
            projectNodeRefs.current.delete(group.id);
          }
        }}
        style={style}
      >
      <div className="project-header-wrapper" style={{ position: "relative" }}>
        <button
          className={`project-button ${hasActiveSession ? "active" : ""}`}
          onPointerDown={(event) => {
            beginProjectPointerTracking(group, event);
          }}
          onClick={(event) => {
            if (suppressProjectClickRef.current) {
              event.preventDefault();
              return;
            }
            setExpandedProjectIds((current) =>
              current.includes(group.id)
                ? current.filter((id) => id !== group.id)
                : [...current, group.id]
            );
          }}
          type="button"
        >
          <Folder size={16} />
          <span>
            {group.name}
            <em>{group.sessions.length}</em>
          </span>
          <ChevronDown className={expanded ? "expanded" : ""} size={15} />
        </button>
        <div className="project-actions-overlay">
          <button
            onClick={(e) => {
              e.stopPropagation();
              onCreateSession(group.id);
            }}
            type="button"
            className="action-icon-btn"
            title={t("新建对话", "New chat")}
          >
            <Plus size={13} />
          </button>
        </div>
      </div>
        <div
          className={`project-sessions-shell ${expanded ? "expanded" : "collapsed"}`}
          aria-hidden={!expanded}
        >
          <div className="project-sessions-inner">
            <div className="project-sessions">
              {group.sessions.map(renderSessionItem)}
            </div>
            {group.sessions.length === 0 ? (
              <div className="project-empty">{t("暂无会话", "No chats yet")}</div>
            ) : null}
          </div>
        </div>
      </div>
    );
  };

  return (
    <aside className="sidebar" style={{ position: "relative" }}>
      <div className="brand-row" data-tauri-drag-region>
        <div className="brand-mark">Y</div>
        <div data-tauri-drag-region>
          <div className="brand-title" data-tauri-drag-region>Yode</div>
          <div className="brand-subtitle" data-tauri-drag-region>local agent runtime</div>
        </div>
      </div>

      <button className="primary-action" onClick={() => onCreateSession()} type="button">
        <MessageSquarePlus size={17} />
        {t("新对话", "New chat")}
      </button>

      <nav className="nav-block" aria-label="主导航">
        <NavButton icon={<Search size={16} />} label={t("搜索", "Search")} />
        <NavButton icon={<Code2 size={16} />} label={t("技能", "Skills")} />
        <NavButton icon={<Workflow size={16} />} label={t("插件", "Plugins")} />
        <NavButton icon={<Clock3 size={16} />} label={t("自动化", "Autopilot")} />
      </nav>

      <div className="sidebar-section sessions">
        <div className="section-head">
          <div className="section-label">{t("项目与对话", "Projects & Chats")}</div>
          <button className="section-action" type="button" onClick={() => void onAddProject()}>
            <FolderPlus size={14} />
            {t("添加项目", "Add project")}
          </button>
        </div>
        <div className="sessions-list">
          {projectGroups.map(renderProjectGroup)}
          {standaloneSessions.length > 0 ? (
            <div className="standalone-group">
              <div className="standalone-label">{t("独立对话", "Standalone")}</div>
              {standaloneSessions.map(renderSessionItem)}
            </div>
          ) : null}
        </div>
      </div>

      {/* Hover info popover card */}
      {hoveredSessionId && hoverPosition && createPortal(
        <div
          className="session-popover"
          style={{
            position: "fixed",
            top: hoverPosition.top,
            left: hoverPosition.left,
            zIndex: 9999,
            width: "220px",
            background: "var(--panel-raised)",
            border: "1px solid var(--line)",
            borderRadius: "var(--radius)",
            padding: "10px",
            boxShadow: "var(--shadow-raised)",
            color: "var(--text)",
            pointerEvents: "none",
            animation: "fadeIn 0.15s ease-out"
          }}
        >
          {(() => {
            const s = sessions.find(x => x.id === hoveredSessionId);
            if (!s) return null;
            return (
              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <div style={{ fontSize: "12px", fontWeight: "700", color: "var(--accent)" }}>
                  {s.title}
                </div>
                <div style={{ display: "flex", flexDirection: "column", gap: "3px", fontSize: "10.5px", color: "var(--text-muted)" }}>
                  <div>
                    <span style={{ color: "var(--text-soft)" }}>{t("项目：", "Project: ")}</span>
                    <code>{s.project || (s.projectRoot ? projectLabelFromPath(s.projectRoot) : t("独立对话", "Standalone"))}</code>
                  </div>
                  <div>
                    <span style={{ color: "var(--text-soft)" }}>{t("更新时间：", "Updated: ")}</span>
                    {s.updatedAt}
                  </div>
                  <div>
                    <span style={{ color: "var(--text-soft)" }}>{t("会话 ID：", "Session ID: ")}</span>
                    <span style={{ fontFamily: "var(--font-code)", opacity: 0.8 }}>{s.id}</span>
                  </div>
                </div>
              </div>
            );
          })()}
        </div>,
        document.body
      )}

      {dragGhost && createPortal(
        <div
          className={`project-drag-ghost ${dragGhost.expanded ? "expanded" : ""}`}
          style={{
            left: dragGhost.left,
            top: dragGhost.y,
            width: dragGhost.width,
            height: dragGhost.height
          }}
        >
          <div className="project-drag-ghost-head">
            <Folder size={16} />
            <span>
              {dragGhost.name}
              <em>{dragGhost.count}</em>
            </span>
          </div>
          {dragGhost.expanded ? (
            <div className="project-drag-ghost-sessions">
              {dragGhost.sessions.length > 0 ? (
                dragGhost.sessions.map((session) => (
                  <div className="project-drag-ghost-session" key={session.id}>
                    <span>{session.title}</span>
                    <em>{session.updatedAt}</em>
                  </div>
                ))
              ) : (
                <div className="project-drag-ghost-empty">{t("暂无会话", "No chats yet")}</div>
              )}
            </div>
          ) : null}
        </div>,
        document.body
      )}

      <div className="sidebar-footer">
        <button
          className={`footer-button ${viewMode === "settings" ? "active" : ""}`}
          onClick={() => onChangeView("settings")}
          type="button"
          title={t("设置", "Settings")}
        >
          <Settings size={17} />
          {t("设置", "Settings")}
        </button>
      </div>
    </aside>
  );
}


function NavButton({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <button className="nav-button" type="button">
      {icon}
      {label}
    </button>
  );
}

function Topbar({
  bootstrap,
  sessionTitle,
  workspacePath,
  inspectorOpen,
  isProcessing,
  onToggleInspector,
  terminalOpen,
  onToggleTerminal,
  currentProvider,
  currentModel,
  onProviderChange,
  onModelChange
}: {
  bootstrap: Bootstrap;
  sessionTitle: string;
  workspacePath: string | null;
  inspectorOpen: boolean;
  isProcessing: boolean;
  onToggleInspector: () => void;
  terminalOpen: boolean;
  onToggleTerminal: () => void;
  currentProvider: string;
  currentModel: string;
  onProviderChange: (provider: string) => void;
  onModelChange: (model: string) => void;
}) {
  const providerOptions = useMemo(() => {
    const saved = localStorage.getItem("yode-llm-providers");
    let list: any[] = [];
    if (saved) {
      try {
        const data = JSON.parse(saved);
        if (Array.isArray(data)) {
          list = data;
        } else if (data && typeof data === "object") {
          list = Object.values(data);
        }
      } catch (e) {}
    }
    const enabledProviders = list.filter((p: any) => p && p.enabled);
    if (enabledProviders.length === 0) {
      return PROVIDERS_META.map((p) => ({
        value: p.id,
        label: p.nameEn
      }));
    }
    return enabledProviders.map((p: any) => ({
      value: p.id,
      label: p.name || p.id
    }));
  }, []);

  return (
    <header className="topbar" data-tauri-drag-region>
      <div className="title-stack" data-tauri-drag-region>
        <div className="session-heading" data-tauri-drag-region>{sessionTitle}</div>
        {workspacePath && (
          <div className="workspace-path" data-tauri-drag-region>
            <span data-tauri-drag-region>{workspacePath}</span>
            <span>main</span>
          </div>
        )}
      </div>
      <div className="runtime-strip" aria-label="运行状态" style={{ display: "flex", gap: "8px", alignItems: "center" }}>
        <DropdownPill
          icon={<TopbarProviderIcon id={currentProvider} />}
          label={getProviderName(currentProvider)}
          value={currentProvider}
          options={providerOptions}
          onChange={onProviderChange}
        />
        <button className="icon-button" type="button" data-tauri-no-drag title="更多">
          <MoreHorizontal size={18} />
        </button>
        <button
          className={`icon-button ${terminalOpen ? "active" : ""}`}
          onClick={onToggleTerminal}
          data-tauri-no-drag
          type="button"
          title={terminalOpen ? "收起终端" : "打开终端"}
        >
          <TerminalSquare size={18} />
        </button>
        <button
          className="icon-button"
          onClick={onToggleInspector}
          data-tauri-no-drag
          type="button"
          title={inspectorOpen ? "收起运行详情" : "展开运行详情"}
        >
          {inspectorOpen ? <PanelRightClose size={18} /> : <PanelRight size={18} />}
        </button>
      </div>
    </header>
  );
}

function TopbarProviderIcon({ id }: { id: string }) {
  const [failed, setFailed] = useState(false);
  if (failed) {
    return <span style={{ width: "14px", height: "14px", display: "inline-block" }} />;
  }
  const aliases: Record<string, string> = {
    baidu: "baidu-qianfan",
    ali: "dashscope-coding",
    qwen: "qwen",
    google: "gemini"
  };
  const iconId = aliases[id] || id;
  const src = `/provider-icons/${iconId}.png`;
  return (
    <img
      src={src}
      alt=""
      style={{ width: "14px", height: "14px", objectFit: "contain", borderRadius: "2px", display: "block" }}
      onError={() => setFailed(true)}
    />
  );
}

function getProviderName(providerId: string) {
  const saved = localStorage.getItem("yode-llm-providers");
  if (saved) {
    try {
      const data = JSON.parse(saved);
      const list = Array.isArray(data) ? data : Object.values(data);
      const found = list.find((p: any) => p.id === providerId);
      if (found && found.name) {
        return found.name;
      }
    } catch (e) {}
  }
  const preset = PROVIDERS_META.find(p => p.id === providerId);
  return preset?.name || providerId;
}

function DropdownPill({
  icon,
  label,
  options,
  value,
  onChange,
  disabled
}: {
  icon: React.ReactNode;
  label: string;
  options: { value: string; label: string }[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  return (
    <div ref={ref} style={{ position: "relative" }}>
      <button
        type="button"
        data-tauri-no-drag
        disabled={disabled}
        onClick={() => setIsOpen(!isOpen)}
        className="status-pill quiet"
        style={{
          cursor: disabled ? "default" : "pointer",
          display: "flex",
          alignItems: "center",
          gap: "6px",
          border: "none",
          background: "var(--field)",
          padding: "4px 8px",
          borderRadius: "var(--radius)",
          color: "var(--text-soft)",
          fontSize: "12px",
          transition: "background 150ms, color 150ms"
        }}
        onMouseEnter={(e) => {
          if (!disabled) {
            e.currentTarget.style.background = "color-mix(in oklch, var(--accent-muted), transparent 60%)";
            e.currentTarget.style.color = "var(--text)";
          }
        }}
        onMouseLeave={(e) => {
          if (!disabled) {
            e.currentTarget.style.background = "var(--field)";
            e.currentTarget.style.color = "var(--text-soft)";
          }
        }}
      >
        {icon}
        <span>{label}</span>
        {!disabled && <ChevronDown size={11} style={{ opacity: 0.7, transform: isOpen ? "rotate(180deg)" : "none", transition: "transform 150ms" }} />}
      </button>

      {isOpen && (
        <div
          className="context-dropdown"
          style={{
            position: "absolute",
            top: "calc(100% + 6px)",
            bottom: "auto",
            left: 0,
            width: "200px"
          }}
        >
          {options.map((opt) => {
            const isSelected = opt.value === value;
            return (
              <button
                key={opt.value}
                type="button"
                data-tauri-no-drag
                className={`context-option ${isSelected ? "selected" : ""}`}
                onClick={() => {
                  onChange(opt.value);
                  setIsOpen(false);
                }}
              >
                <TopbarProviderIcon id={opt.value} />
                <span>{opt.label}</span>
                {isSelected ? <Check size={14} style={{ color: "var(--accent)" }} /> : <span />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

function StatusPill({
  icon,
  label,
  tone
}: {
  icon: React.ReactNode;
  label: string;
  tone?: "live" | "quiet";
}) {
  return (
    <span className={`status-pill ${tone ?? ""}`}>
      {icon}
      {label}
    </span>
  );
}

function ChatWorkspace({
  draft,
  timelineItems,
  onDraftChange,
  onSendMessage,
  inspectorOpen,
  isProcessing,
  onCancelMessage,
  permissionMode,
  onPermissionModeChange,
  onPermissionResolved,
  appLang,
  projectOptions,
  selectedProjectRoot,
  onProjectRootChange,
  onAddProject,
  currentProvider,
  currentModel,
  onModelChange
}: {
  draft: string;
  timelineItems: TimelineItem[];
  onDraftChange: (value: string) => void;
  onSendMessage: () => void;
  inspectorOpen: boolean;
  isProcessing: boolean;
  onCancelMessage: () => void;
  permissionMode: string;
  onPermissionModeChange: (mode: string) => void;
  onPermissionResolved: (id: string) => void;
  appLang: string;
  projectOptions: Array<{ label: string; root: string | null }>;
  selectedProjectRoot: string | null;
  onProjectRootChange: (root: string | null) => void;
  onAddProject: () => Promise<void>;
  currentProvider: string;
  currentModel: string;
  onModelChange: (model: string) => void;
}) {
  // Check if assistant is currently streaming (has any running status or last item kind is not fully completed)
  const isStreaming = useMemo(() => {
    if (isProcessing) return true;

    // If there is any item with status === 'running', it is still streaming/running.
    const hasRunningTool = timelineItems.some(item => item.kind === "tool" && item.status === "running");
    if (hasRunningTool) return true;
    
    const lastItem = timelineItems[timelineItems.length - 1];
    if (!lastItem) return false;
    if (lastItem.kind === "reasoning" && lastItem.meta === "running") {
      return true;
    }
    if (lastItem.kind === "assistant" && lastItem.meta !== "stream complete") {
      return true;
    }
    return false;
  }, [timelineItems, isProcessing]);

  // Track expanded turn IDs separately
  const [expandedTurnIds, setExpandedTurnIds] = useState<string[]>([]);
  const [expandedActionIds, setExpandedActionIds] = useState<string[]>([]);

  const turns = useMemo(() => {
    const list: Array<{
      id: string;
      userItem: TimelineItem | null;
      items: TimelineItem[];
      hasIntermediate: boolean;
    }> = [];

    let currentTurn: typeof list[number] = {
      id: "welcome",
      userItem: null,
      items: [],
      hasIntermediate: false
    };

    timelineItems.forEach((item) => {
      if (item.kind === "user") {
        if (currentTurn.userItem || currentTurn.items.length > 0) {
          list.push(currentTurn);
        }
        currentTurn = {
          id: item.id,
          userItem: item,
          items: [],
          hasIntermediate: false
        };
      } else {
        currentTurn.items.push(item);
        if (
          item.kind === "tool" ||
          item.kind === "reasoning" ||
          (item as any).kind === "process_note" ||
          (item.kind === "assistant" && isIntermediateAssistantItem(item))
        ) {
          currentTurn.hasIntermediate = true;
        }
      }
    });

    if (currentTurn.userItem || currentTurn.items.length > 0) {
      list.push(currentTurn);
    }

    return list;
  }, [timelineItems]);

  const activePermission = [...timelineItems]
    .reverse()
    .find((item): item is Extract<TimelineItem, { kind: "permission" }> => item.kind === "permission");

  useEffect(() => {
    if (isStreaming && turns.length > 0) {
      const lastTurnId = turns[turns.length - 1].id;
      setExpandedTurnIds((prev) => {
        if (prev.includes(lastTurnId)) return prev;
        return [...prev, lastTurnId];
      });
    }
  }, [isStreaming, turns]);

  const timelinePanelRef = useRef<HTMLElement | null>(null);
  const shouldStickToBottomRef = useRef(true);
  const lastTimelineLengthRef = useRef(0);

  const scrollTimelineToBottom = (behavior: ScrollBehavior = "smooth") => {
    const panel = timelinePanelRef.current;
    if (!panel) return;
    panel.scrollTo({
      top: panel.scrollHeight,
      behavior
    });
  };

  const handleTimelineScroll = () => {
    const panel = timelinePanelRef.current;
    if (!panel) return;
    const distanceToBottom = panel.scrollHeight - panel.scrollTop - panel.clientHeight;
    shouldStickToBottomRef.current = distanceToBottom < 120;
  };

  useLayoutEffect(() => {
    if (!shouldStickToBottomRef.current) return;
    const itemAdded = timelineItems.length > lastTimelineLengthRef.current;
    lastTimelineLengthRef.current = timelineItems.length;
    const frame = window.requestAnimationFrame(() => {
      scrollTimelineToBottom(itemAdded && !isStreaming ? "smooth" : "auto");
    });
    return () => window.cancelAnimationFrame(frame);
  }, [timelineItems.length, isStreaming]);

  return (
    <div className={`chat-layout ${inspectorOpen ? "" : "inspector-collapsed"}`}>
      <div className="conversation-column">
        <section
          className="timeline-panel"
          aria-label="会话时间线"
          ref={timelinePanelRef}
          onScroll={handleTimelineScroll}
        >
          {/* Removed RUN LOG header to clean up space */}
          
          {turns.length === 0 ? (
            <div className="welcome-dashboard">
              <div className="welcome-logo">
                <Bot size={44} className="glowing-logo" style={{ color: "var(--accent)" }} />
              </div>
              <h1 className="welcome-title">{appLang === "zh" ? "今天想构建点什么？" : "What would you like to build today?"}</h1>
              <p className="welcome-subtitle">
                {appLang === "zh" 
                  ? "输入仓库任务以开始，我将帮助你分析、编写和调试代码。" 
                  : "Enter a repository task to start. I'll help you analyze, write, and debug code."}
              </p>
              
              <div className="welcome-cards">
                <button 
                  type="button"
                  className="welcome-card"
                  onClick={() => onDraftChange(appLang === "zh" ? "解释当前项目的主要架构 and 目录结构" : "Explain the main architecture and directory structure of this project")}
                >
                  <h3>🔍 {appLang === "zh" ? "分析项目" : "Analyze Project"}</h3>
                  <p>{appLang === "zh" ? "解释当前项目的主要架构和目录结构" : "Explain the main architecture and directory structure of this project"}</p>
                </button>
                <button 
                  type="button"
                  className="welcome-card"
                  onClick={() => onDraftChange(appLang === "zh" ? "帮我找出当前代码中可以优化性能的模块" : "Help me find modules in the current code that can be optimized for performance")}
                >
                  <h3>🛠️ {appLang === "zh" ? "代码优化" : "Code Optimization"}</h3>
                  <p>{appLang === "zh" ? "帮我找出当前代码中可以优化性能的模块" : "Help me find modules in the current code that can be optimized for performance"}</p>
                </button>
                <button 
                  type="button"
                  className="welcome-card"
                  onClick={() => onDraftChange(appLang === "zh" ? "为最近修改的 Rust 模块生成单元测试" : "Generate unit tests for the recently modified Rust modules")}
                >
                  <h3>📝 {appLang === "zh" ? "编写测试" : "Write Tests"}</h3>
                  <p>{appLang === "zh" ? "为最近修改的 Rust 模块生成单元测试" : "Generate unit tests for the recently modified Rust modules"}</p>
                </button>
              </div>
            </div>
          ) : (
            turns.map((turn, turnIndex) => {
              const isLastTurn = turnIndex === turns.length - 1;
              const isTurnActive = isStreaming && isLastTurn;
              const visibleItems = compileInlineItems(turn.items, isTurnActive, appLang);

              return (
                <React.Fragment key={turn.id}>
                  {turn.userItem && <TimelineNode item={turn.userItem} appLang={appLang} isTurnActive={isTurnActive} />}
                  {visibleItems.map((item) => (
                    <TimelineNode key={item.id} item={item} appLang={appLang} isTurnActive={isTurnActive} />
                  ))}
                </React.Fragment>
              );
            })
          )}
        </section>
        {activePermission ? (
          <div className="permission-dock" aria-label="执行确认">
            <PermissionActions
              item={activePermission}
              appLang={appLang}
              onResolved={() => onPermissionResolved(activePermission.id)}
            />
          </div>
        ) : null}
        <Composer
          draft={draft}
          onDraftChange={onDraftChange}
          onSendMessage={onSendMessage}
          isProcessing={isProcessing}
          onCancelMessage={onCancelMessage}
          permissionMode={permissionMode}
          onPermissionModeChange={onPermissionModeChange}
          appLang={appLang}
          projectOptions={projectOptions}
          selectedProjectRoot={selectedProjectRoot}
          onProjectRootChange={onProjectRootChange}
          onAddProject={onAddProject}
          currentProvider={currentProvider}
          currentModel={currentModel}
          onModelChange={onModelChange}
        />
      </div>
      <RunInspector
        isProcessing={isProcessing}
        permissionMode={permissionMode}
        timelineItems={timelineItems}
      />
    </div>
  );
}

function LiveDurationHeader({ turnId, isActive, isExpanded, onToggle, staticDuration, startTime }: {
  turnId: string;
  isActive: boolean;
  isExpanded: boolean;
  onToggle: () => void;
  staticDuration: number;
  startTime?: number;
}) {
  const [elapsed, setElapsed] = useState(staticDuration);
  const startRef = useRef<number | null>(null);

  useEffect(() => {
    if (!isActive) {
      setElapsed(staticDuration);
      return;
    }

    if (startRef.current === null) {
      startRef.current = startTime || Date.now();
    }
    const start = startRef.current;

    setElapsed(Math.floor((Date.now() - start) / 1000));

    const timer = setInterval(() => {
      setElapsed(Math.floor((Date.now() - start) / 1000));
    }, 1000);

    return () => clearInterval(timer);
  }, [turnId, isActive, staticDuration, startTime]);


  let durationText: string;
  if (elapsed < 60) {
    durationText = `${elapsed}s`;
  } else {
    durationText = `${Math.floor(elapsed / 60)}m ${elapsed % 60}s`;
  }

  return (
    <div
      onClick={onToggle}
      style={{
        background: "transparent",
        border: "none",
        cursor: "pointer",
        fontSize: "12px",
        color: "var(--text-soft)",
        display: "inline-flex",
        alignItems: "center",
        gap: "6px",
        userSelect: "none",
      }}
      onMouseEnter={(e) => { e.currentTarget.style.color = "var(--text)"; }}
      onMouseLeave={(e) => { e.currentTarget.style.color = "var(--text-soft)"; }}
    >
      <span>{`Worked for ${durationText}`}</span>
      <ChevronDown size={13} style={{
        opacity: 0.7,
        transform: isExpanded ? "none" : "rotate(-90deg)",
        transition: "transform 150ms ease"
      }} />
    </div>
  );
}

function InlineToolGroup({ label, items, appLang }: { label: string; items: any[]; appLang: string }) {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div style={{
      maxWidth: "760px",
      width: "100%",
      margin: "4px auto 8px",
      paddingLeft: "33px",
      fontSize: "12px",
      color: "var(--text-soft)",
      userSelect: "none"
    }}>
      <div 
        onClick={() => setIsExpanded(!isExpanded)}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "6px",
          cursor: "pointer",
          transition: "color 0.15s ease",
        }}
        onMouseEnter={(e) => { e.currentTarget.style.color = "var(--text)"; }}
        onMouseLeave={(e) => { e.currentTarget.style.color = "var(--text-soft)"; }}
      >
        <span>{label}</span>
        <ChevronDown size={11} style={{
          opacity: 0.7,
          transform: isExpanded ? "none" : "rotate(-90deg)",
          transition: "transform 150ms ease"
        }} />
      </div>
      
      {isExpanded && items && items.length > 0 && (
        <div style={{
          marginTop: "6px",
          paddingLeft: "10px",
          borderLeft: "1.5px solid var(--line-soft)",
          display: "flex",
          flexDirection: "column",
          gap: "6px",
          fontSize: "11.5px"
        }}>
          {items.map((item, idx) => (
            <div key={idx} style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
              <div style={{ fontWeight: "600", color: "var(--text)" }}>{item.title}</div>
              {item.body && (
                <pre style={{
                  margin: "2px 0 0",
                  padding: "6px",
                  background: "color-mix(in oklch, var(--field), transparent 4%)",
                  borderRadius: "4px",
                  overflowX: "auto",
                  maxHeight: "80px",
                  whiteSpace: "pre-wrap",
                  fontFamily: "var(--font-code)",
                  fontSize: "10.5px",
                  color: "var(--text-muted)",
                  border: "1px solid var(--line-soft)"
                }}>
                  {item.body.length > 300 ? item.body.substring(0, 300) + "..." : item.body}
                </pre>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function TimelineNode({ item, appLang, isTurnActive }: { item: TimelineItem; appLang: string; isTurnActive?: boolean }) {
  if (item.kind === "boundary" || item.kind === "permission") {
    return null;
  }

  if (item.kind === "process_note") {
    return <ProcessNoteNode note={item} appLang={appLang} />;
  }

  if (item.kind === "activity_group") {
    return (
      <ActivityGroupNode 
        group={item} 
        appLang={appLang} 
        isTurnActive={isTurnActive}
      />
    );
  }

  if (item.kind === "activity_item") {
    return (
      <ActivityItemNode 
        node={item} 
        appLang={appLang} 
      />
    );
  }

  if (item.kind === "reasoning" && item.id === "retrying-attempt") {
    return (
      <div 
        style={{ 
          maxWidth: "760px",
          width: "100%",
          margin: "8px auto 12px",
          padding: "8px 0 8px 33px",
          fontSize: "12px",
          display: "flex",
          flexDirection: "column",
          gap: "4px",
          userSelect: "none"
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "6px", fontWeight: "500", color: "var(--warning)" }}>
          <Clock3 size={13} style={{ animation: "pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite" }} />
          <span>{item.title}</span>
        </div>
        {item.body && (
          <div style={{ color: "var(--text-soft)", fontSize: "11.5px", whiteSpace: "pre-wrap", marginTop: "2px" }}>
            {item.body}
          </div>
        )}
      </div>
    );
  }

  if (item.kind === "tool" || item.kind === "reasoning") {
    return null;
  }

  if (item.kind === "tool_group") {
    return (
      <InlineToolGroup 
        label={item.label} 
        items={item.items || []} 
        appLang={appLang} 
      />
    );
  }

  if (item.kind === "user") {
    return (
      <div 
        className="timeline-node user-bubble-container" 
        style={{ 
          display: "flex", 
          justifyContent: "flex-end", 
          width: "100%", 
          maxWidth: "760px",
          margin: "0 auto 12px",
          paddingLeft: "24px"
        }}
      >
        <div 
          className="user-chat-bubble"
          style={{
            background: "color-mix(in oklch, var(--accent), transparent 85%)",
            border: "none",
            borderRadius: "14px 14px 2px 14px",
            padding: "10px 14px",
            maxWidth: "85%",
            boxShadow: "0 2px 8px rgba(0, 0, 0, 0.15)",
            display: "block",
            overflow: "hidden"
          }}
        >
          <p style={{ 
            margin: 0, 
            color: "var(--text)", 
            fontSize: "13px", 
            lineHeight: "1.45", 
            whiteSpace: "pre-wrap",
            wordBreak: "break-word"
          }}>
            {item.body}
          </p>
        </div>
      </div>
    );
  }

  // Intermediate assistant messages: render as clean inline text (no icon, no header)
  if (item.kind === "assistant" && (item.meta === "intermediate" || item.meta === "streaming")) {
    return (
      <div 
        style={{ 
          maxWidth: "760px",
          width: "100%",
          margin: "4px auto 12px",
          paddingLeft: "33px",
        }}
      >
        <MarkdownContent text={item.body} />
      </div>
    );
  }

  if (item.kind !== "assistant") {
    return null;
  }

  const icon = <Bot size={18} />;

  return (
    <article className={`timeline-node ${item.kind}`}>
      <div className="node-rail">
        <div className="node-icon">{icon}</div>
      </div>
      <div className="node-content">
        <div className="node-header">
          <h2>{item.title}</h2>
          {"meta" in item && item.meta ? <span>{item.meta}</span> : null}
        </div>
        {item.kind === "assistant" ? (
          <MarkdownContent text={item.body} />
        ) : "body" in item ? (
          <p>{(item as any).body}</p>
        ) : null}
      </div>
    </article>
  );
}

function CodeBlock({ text, lang }: { text: string; lang: string }) {
  const [copied, setCopied] = useState(false);

  // Check for truncated message line
  const lines = text.split("\n");
  const truncateIndex = lines.findIndex(line => 
    line.includes("Output truncated by runtime guard") || 
    line.includes("truncated by runtime guard")
  );

  let cleanText = text;
  let isTruncated = false;
  if (truncateIndex !== -1) {
    isTruncated = true;
    cleanText = lines.slice(0, truncateIndex).join("\n");
  }

  const highlighted = useMemo(() => {
    try {
      if (lang && hljs.getLanguage(lang)) {
        return hljs.highlight(cleanText, { language: lang, ignoreIllegals: true }).value;
      }
      return hljs.highlightAuto(cleanText).value;
    } catch (e) {
      return cleanText
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
    }
  }, [cleanText, lang]);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(cleanText);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [cleanText]);

  return (
    <div className="code-block-container" style={{ 
      margin: "12px 0", 
      border: "1px solid var(--line-soft)", 
      borderRadius: "var(--radius)", 
      overflow: "hidden", 
      background: "color-mix(in oklch, var(--field), transparent 8%)",
      display: "flex",
      flexDirection: "column"
    }}>
      <div className="code-block-header" style={{ 
        display: "flex", 
        justifyContent: "space-between", 
        alignItems: "center", 
        padding: "6px 12px", 
        borderBottom: "1px solid var(--line-soft)", 
        background: "color-mix(in oklch, var(--field), transparent 4%)", 
        fontSize: "11px", 
        color: "var(--text-soft)",
        userSelect: "none"
      }}>
        <span style={{ fontFamily: "var(--font-code)", textTransform: "uppercase" }}>{lang || "code"}</span>
        <button onClick={handleCopy} style={{ 
          display: "flex", 
          alignItems: "center", 
          gap: "4px", 
          background: "transparent", 
          border: "none", 
          color: "var(--text-soft)", 
          cursor: "pointer", 
          padding: "2px 6px", 
          borderRadius: "4px" 
        }}>
          {copied ? <Check size={12} /> : <Copy size={12} />}
          <span>{copied ? "已复制" : "复制"}</span>
        </button>
      </div>
      <pre className="hljs" style={{ 
        margin: 0, 
        padding: "12px", 
        overflowX: "auto", 
        display: "block", 
        background: "transparent",
        whiteSpace: "pre",
        wordBreak: "normal",
        wordWrap: "normal"
      }}>
        <code dangerouslySetInnerHTML={{ __html: highlighted }} style={{
          border: 0,
          padding: 0,
          background: "transparent",
          fontFamily: "var(--font-code)",
          fontSize: "12px",
          lineHeight: "1.5"
        }} />
      </pre>
      {isTruncated && (
        <div style={{ 
          padding: "8px 12px", 
          borderTop: "1px solid var(--line-soft)", 
          background: "color-mix(in oklch, var(--warning), transparent 95%)", 
          color: "var(--text-soft)", 
          fontSize: "11.5px", 
          display: "flex", 
          alignItems: "center", 
          gap: "6px" 
        }}>
          <AlertCircle size={13} style={{ color: "var(--warning)" }} />
          <span>输出已被安全守护截断，可输入“继续”以获取完整内容。</span>
        </div>
      )}
    </div>
  );
}

function isIntermediateAssistantItem(item: TimelineItem) {
  if (item.kind === "assistant") {
    const body = item.body.trim();
    return item.meta === "intermediate" || body === "" || body === "." || body === "..." || body === "…";
  }
  return false;
}

function MarkdownContent({ text }: { text: string }) {
  const blocks = useMemo(() => parseMarkdownBlocks(text), [text]);
  return (
    <div className="markdown-content">
      {blocks.map((block, index) => {
        if (block.type === "heading") {
          const Tag = `h${Math.min(block.level, 4)}` as keyof JSX.IntrinsicElements;
          return <Tag key={index}>{renderInlineMarkdown(block.text)}</Tag>;
        }
        if (block.type === "code") {
          return <CodeBlock key={index} text={block.text} lang={block.lang} />;
        }
        if (block.type === "list") {
          return (
            <ul key={index}>
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
              ))}
            </ul>
          );
        }
        if (block.type === "table") {
          return (
            <div key={index} className="markdown-table-wrapper" style={{ overflowX: "auto", margin: "12px 0" }}>
              <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "12px" }}>
                <thead>
                  <tr style={{ borderBottom: "2px solid var(--line)" }}>
                    {block.headers.map((h, i) => (
                      <th key={i} style={{ padding: "8px", textAlign: "left", fontWeight: "bold" }}>
                        {renderInlineMarkdown(h)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {block.rows.map((row, ri) => (
                    <tr key={ri} style={{ borderBottom: "1px solid var(--line-soft)" }}>
                      {row.map((cell, ci) => (
                        <td key={ci} style={{ padding: "8px" }}>
                          {renderInlineMarkdown(cell)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        }
        if (block.type === "divider") {
          return <hr key={index} style={{ border: "0", borderTop: "1px solid var(--line-soft)", margin: "16px 0" }} />;
        }
        return <p key={index}>{renderInlineMarkdown(block.text)}</p>;
      })}
    </div>
  );
}

type MarkdownBlock =
  | { type: "heading"; level: number; text: string }
  | { type: "code"; text: string; lang: string }
  | { type: "list"; items: string[] }
  | { type: "table"; headers: string[]; rows: string[][] }
  | { type: "divider" }
  | { type: "paragraph"; text: string };

function parseMarkdownBlocks(text: string): MarkdownBlock[] {
  const blocks: MarkdownBlock[] = [];
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  let paragraph: string[] = [];
  let list: string[] = [];
  let tableRows: string[][] = [];
  let code: string[] | null = null;
  const flushParagraph = () => {
    if (paragraph.length > 0) {
      blocks.push({ type: "paragraph", text: paragraph.join(" ") });
      paragraph = [];
    }
  };
  const flushList = () => {
    if (list.length > 0) {
      blocks.push({ type: "list", items: list });
      list = [];
    }
  };
  const flushTable = () => {
    if (tableRows.length > 0) {
      if (tableRows.length >= 2 && tableRows[1].every(cell => /^:?-+:?$/.test(cell.trim()))) {
        const headers = tableRows[0];
        const rows = tableRows.slice(2);
        blocks.push({ type: "table", headers, rows });
      } else {
        for (const row of tableRows) {
          paragraph.push("|" + row.join("|") + "|");
        }
      }
      tableRows = [];
    }
  };
  let codeLang = "";
  for (const line of lines) {
    const fenceMatch = line.trim().match(/^```(.*)$/);
    if (fenceMatch) {
      if (code) {
        blocks.push({ type: "code", text: code.join("\n"), lang: codeLang });
        code = null;
        codeLang = "";
      } else {
        flushParagraph();
        flushList();
        flushTable();
        codeLang = fenceMatch[1].trim().toLowerCase();
        code = [];
      }
      continue;
    }

    if (code) {
      code.push(line);
      continue;
    }

    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) {
      flushParagraph();
      flushList();
      flushTable();
      blocks.push({ type: "heading", level: heading[1].length, text: heading[2].trim() });
      continue;
    }

    const listItem = line.match(/^\s*[-*]\s+(.+)$/);
    if (listItem) {
      flushParagraph();
      flushTable();
      list.push(listItem[1].trim());
      continue;
    }

    const isDivider = /^(?:-{3,}|\*{3,}|_{3,})$/.test(line.trim());
    if (isDivider) {
      flushParagraph();
      flushList();
      flushTable();
      blocks.push({ type: "divider" });
      continue;
    }

    const isTableRow = line.trim().startsWith("|") && line.trim().endsWith("|");
    if (isTableRow) {
      flushParagraph();
      flushList();
      const cells = line.split("|").map(c => c.trim()).slice(1, -1);
      tableRows.push(cells);
      continue;
    }

    if (!line.trim()) {
      flushParagraph();
      flushList();
      flushTable();
      continue;
    }

    flushList();
    flushTable();
    paragraph.push(line.trim());
  }

  if (code) blocks.push({ type: "code", text: code.join("\n"), lang: codeLang });
  flushParagraph();
  flushList();
  flushTable();
  return blocks.length > 0 ? blocks : [{ type: "paragraph", text }];
}

function renderInlineMarkdown(text: string) {
  const parts = text.split(/(`[^`]+`|\*\*[^*]+\*\*)/g).filter(Boolean);
  return parts.map((part, index) => {
    if (part.startsWith("`") && part.endsWith("`")) {
      return <code key={index}>{part.slice(1, -1)}</code>;
    }
    if (part.startsWith("**") && part.endsWith("**")) {
      return <strong key={index}>{part.slice(2, -2)}</strong>;
    }
    return <React.Fragment key={index}>{part}</React.Fragment>;
  });
}

function ToolMeta({ item }: { item: Extract<TimelineItem, { kind: "tool" }> }) {
  return (
    <div className="tool-meta">
      <span className={`tool-state ${item.status}`}>{statusLabel(item.status)}</span>
      <code>{item.tool}</code>
      <button className="ghost-button" type="button">
        open
      </button>
    </div>
  );
}

function PermissionActions({
  item,
  appLang,
  onResolved
}: {
  item: Extract<TimelineItem, { kind: "permission" }>;
  appLang: string;
  onResolved?: () => void;
}) {
  const isZh = appLang === "zh";

  const options = [
    {
      id: "allow_once",
      label: isZh ? "允许本次执行" : "Yes, allow this time",
      description: isZh ? "仅允许本次执行" : "Only allow this execution"
    },
    {
      id: "always_allow",
      label: isZh ? "总是允许此命令" : "Yes, always allow this command",
      description: isZh ? "后续同类命令不再询问" : "Do not ask again for similar commands"
    },
    {
      id: "deny",
      label: isZh ? "拒绝并改用其他方式" : "No",
      description: isZh ? "告诉 agent 改用其他方式" : "Tell agent to use another way"
    }
  ] as const;

  const [selectedIndex, setSelectedIndex] = useState(0);
  const selectedOption = options[selectedIndex];
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const respond = (decision: (typeof options)[number]["id"]) => {
    onResolved?.();
    if (item.sessionId && item.turnId) {
      invoke("permission_respond", {
        sessionId: item.sessionId,
        turnId: item.turnId,
        allow: decision !== "deny",
        alwaysAllow: decision === "always_allow"
      }).catch(console.error);
    }
  };

  useEffect(() => {
    optionRefs.current[selectedIndex]?.focus();
  }, [selectedIndex]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((index) => (index - 1 + options.length) % options.length);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((index) => (index + 1) % options.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        respond(selectedOption.id);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedOption.id, item.sessionId, item.turnId]);

  return (
    <div className="permission-prompt">
      <div className="permission-prompt-title">
        <TerminalSquare size={16} />
        <span>{isZh ? "允许运行此命令吗？" : "Allow running this command?"}</span>
      </div>
      <pre className="permission-command">{item.body || item.tool}</pre>
      <div className="permission-option-list">
        {options.map((option, index) => (
          <button
            className={`permission-option ${selectedIndex === index ? "selected" : ""}`}
            key={option.id}
            ref={(node) => {
              optionRefs.current[index] = node;
            }}
            onClick={() => {
              setSelectedIndex(index);
              respond(option.id);
            }}
            type="button"
            style={{ outline: "none", boxShadow: "none" }}
          >
            <kbd>{index + 1}</kbd>
            <span>{option.label}</span>
            <em>{option.description}</em>
          </button>
        ))}
      </div>
      <div className="permission-prompt-footer">
        <button className="permission-skip" onClick={() => respond("deny")} type="button" style={{ outline: "none", boxShadow: "none" }}>
          {isZh ? "跳过" : "Skip"}
        </button>
        <button className="permission-submit" onClick={() => respond(selectedOption.id)} type="button" style={{ outline: "none", boxShadow: "none" }}>
          {isZh ? "提交" : "Submit"}
          <span>↵</span>
        </button>
      </div>
    </div>
  );
}

function statusLabel(status: "running" | "success" | "blocked") {
  if (status === "running") return "运行中";
  if (status === "success") return "完成";
  return "阻塞";
}

function messagesToTimelineItems(messages: DesktopMessage[]): TimelineItem[] {
  return messages.flatMap((message): TimelineItem[] => {
    const content = message.content?.trim();
    const reasoning = message.reasoning?.trim();
    const timestamp = formatHistoryTimestamp(message.createdAt);

    if (message.role === "user") {
      return content
        ? [{
            id: `history-${message.id}`,
            kind: "user",
            title: "用户",
            body: content,
            meta: timestamp,
            createdAt: new Date(message.createdAt).getTime()
          }]
        : [];
    }

    if (message.role === "assistant") {
      const items: TimelineItem[] = [];
      if (reasoning) {
        items.push({
          id: `history-${message.id}-reasoning`,
          kind: "reasoning",
          title: "已思考",
          body: "",
          meta: "complete"
        });
      }
      if (content) {
        const hasTools = parseToolCalls(message.toolCallsJson).length > 0;
        items.push({
          id: `history-${message.id}`,
          kind: "assistant",
          title: "Yode",
          body: content,
          meta: hasTools ? "intermediate" : "stream complete"
        });
      }
      parseToolCalls(message.toolCallsJson).forEach((toolCall, index) => {
        items.push({
          id: `history-${message.id}-tool-call-${index}`,
          kind: "tool",
          title: `调用工具: ${toolCall.name}`,
          body: toolCall.arguments,
          tool: toolCall.name,
          status: "success",
          meta: "history"
        });
      });
      return items;
    }

    if (message.role === "tool") {
      return [{
        id: `history-${message.id}`,
        kind: "tool",
        title: "工具结果",
        body: content || message.toolCallId || "",
        tool: message.toolCallId || "tool",
        status: "success"
      }];
    }

    return [];
  });
}

function parseToolCalls(raw: string | null | undefined): Array<{ name: string; arguments: string }> {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.flatMap((item) => {
      const name = stringValue(item?.name) ?? stringValue(item?.function?.name);
      const args =
        stringValue(item?.arguments) ??
        stringValue(item?.function?.arguments) ??
        JSON.stringify(item?.arguments ?? item?.function?.arguments ?? {});
      return name ? [{ name, arguments: args }] : [];
    });
  } catch {
    return [];
  }
}

function formatHistoryTimestamp(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return undefined;
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  });
}

function projectLabelFromPath(path: string) {
  const trimmed = path.trim();
  if (!trimmed) return "项目";
  const parts = trimmed.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] || trimmed;
}

function deriveSessionTitle(content: string) {
  const normalized = content.replace(/\s+/g, " ").trim();
  if (!normalized) return "新对话";
  return normalized.length > 28 ? normalized.slice(0, 28) : normalized;
}

function upsertActiveSession(items: SessionSummary[], session: SessionSummary) {
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

function Composer({
  draft,
  onDraftChange,
  onSendMessage,
  isProcessing,
  onCancelMessage,
  permissionMode,
  onPermissionModeChange,
  appLang,
  projectOptions,
  selectedProjectRoot,
  onProjectRootChange,
  onAddProject,
  currentProvider,
  currentModel,
  onModelChange
}: {
  draft: string;
  onDraftChange: (value: string) => void;
  onSendMessage: () => void;
  isProcessing: boolean;
  onCancelMessage: () => void;
  permissionMode: string;
  onPermissionModeChange: (mode: string) => void;
  appLang: string;
  projectOptions: Array<{ label: string; root: string | null }>;
  selectedProjectRoot: string | null;
  onProjectRootChange: (root: string | null) => void;
  onAddProject: () => Promise<void>;
  currentProvider: string;
  currentModel: string;
  onModelChange: (model: string) => void;
}) {
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [projectDropdownOpen, setProjectDropdownOpen] = useState(false);
  const [modelDropdownOpen, setModelDropdownOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const projectDropdownRef = useRef<HTMLDivElement>(null);
  const modelDropdownRef = useRef<HTMLDivElement>(null);

  const isZh = appLang === "zh";

  const modelOptions = useMemo(() => {
    const saved = localStorage.getItem("yode-llm-providers");
    let list: any[] = [];
    if (saved) {
      try {
        const data = JSON.parse(saved);
        if (Array.isArray(data)) {
          list = data;
        } else if (data && typeof data === "object") {
          list = Object.values(data);
        }
      } catch (e) {}
    }
    const found = list.find((p: any) => p && p.id === currentProvider);
    if (found && Array.isArray(found.models) && found.models.length > 0) {
      return found.models;
    }
    const meta = PROVIDERS_META.find((p) => p.id === currentProvider);
    return meta ? meta.defaultModels : [];
  }, [currentProvider]);

  const OPTIONS = [
    {
      key: "default",
      label: isZh ? "每次询问" : "Ask for approval",
      description: isZh ? "修改外部文件及使用网络时，总是需要确认" : "Always ask to edit external files and use the internet",
      icon: <Hand size={15} />
    },
    {
      key: "auto",
      label: isZh ? "自动授权安全操作" : "Approve for me",
      description: isZh ? "仅对检测到存在潜在风险的操作进行询问" : "Only ask for actions detected as potentially unsafe",
      icon: <Shield size={15} />
    },
    {
      key: "bypass",
      label: isZh ? "完全信任" : "Full access",
      description: isZh ? "不受限制地访问网络及您计算机上的任何文件" : "Unrestricted access to the internet and any file on your computer",
      icon: <AlertCircle size={15} />
    }
  ];

  const currentOption = OPTIONS.find(
    (o) => o.key.toLowerCase() === (permissionMode || "default").toLowerCase()
  ) || OPTIONS[0];
  const currentProject =
    selectedProjectRoot === null
      ? projectOptions.find((option) => option.root === null) ?? {
          label: isZh ? "独立对话" : "Standalone",
          root: null
        }
      : projectOptions.find((option) => option.root === selectedProjectRoot) ??
        projectOptions[0] ?? {
          label: isZh ? "当前项目" : "Current project",
          root: selectedProjectRoot ?? null
        };

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setDropdownOpen(false);
      }
      if (
        projectDropdownRef.current &&
        !projectDropdownRef.current.contains(event.target as Node)
      ) {
        setProjectDropdownOpen(false);
      }
      if (
        modelDropdownRef.current &&
        !modelDropdownRef.current.contains(event.target as Node)
      ) {
        setModelDropdownOpen(false);
      }
    }
    if (dropdownOpen || projectDropdownOpen || modelDropdownOpen) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [dropdownOpen, projectDropdownOpen, modelDropdownOpen]);

  return (
    <footer className="composer" style={{ position: "relative" }}>
      <textarea
        aria-label="消息"
        placeholder={isZh ? "输入仓库任务..." : "Enter repository task..."}
        value={draft}
        onChange={(event) => onDraftChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            if (event.metaKey || event.ctrlKey) {
              // Cmd+Enter or Ctrl+Enter -> Newline
              event.preventDefault();
              const target = event.target as HTMLTextAreaElement;
              const start = target.selectionStart;
              const end = target.selectionEnd;
              const val = target.value;
              const nextVal = val.substring(0, start) + "\n" + val.substring(end);
              onDraftChange(nextVal);
              // reset cursor position
              setTimeout(() => {
                target.selectionStart = target.selectionEnd = start + 1;
              }, 0);
            } else {
              // Plain Enter -> Send / Queue
              event.preventDefault();
              onSendMessage();
            }
          }
        }}
      />
      <div className="composer-toolbar">
        <div className="composer-tools" style={{ position: "relative" }}>
          <button className="icon-button" type="button" title={isZh ? "附件" : "Attachment"} style={{ outline: "none", boxShadow: "none" }}>
            <Paperclip size={17} />
          </button>

          <div ref={projectDropdownRef} style={{ display: "inline-block", position: "relative" }}>
            <button
              className="mode-chip"
              type="button"
              onClick={() => setProjectDropdownOpen(!projectDropdownOpen)}
              title={currentProject.root ?? (isZh ? "独立对话" : "Standalone")}
              style={{ outline: "none", boxShadow: "none", cursor: "pointer" }}
            >
              <Folder size={15} />
              {currentProject.label}
            </button>

            {projectDropdownOpen && (
              <div className="context-dropdown project-dropdown">
                {projectOptions.map((option) => {
                  const selected = option.root === selectedProjectRoot;
                  return (
                    <button
                      key={option.root ?? "__standalone__"}
                      type="button"
                      className={`context-option ${selected ? "selected" : ""}`}
                      onClick={() => {
                        onProjectRootChange(option.root);
                        setProjectDropdownOpen(false);
                      }}
                    >
                      <Folder size={14} />
                      <span>{option.label}</span>
                      {selected ? <Check size={14} /> : null}
                    </button>
                  );
                })}
                <div className="context-dropdown-divider" />
                <button
                  type="button"
                  className="context-option context-option-action"
                  onClick={() => {
                    setProjectDropdownOpen(false);
                    void onAddProject();
                  }}
                >
                  <FolderPlus size={14} />
                  <span>{isZh ? "添加项目..." : "Add project..."}</span>
                </button>
              </div>
            )}
          </div>
          
          <div ref={dropdownRef} style={{ display: "inline-block" }}>
            <button
              className="mode-chip"
              type="button"
              onClick={() => setDropdownOpen(!dropdownOpen)}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: "6px",
                cursor: "pointer",
                position: "relative",
                outline: "none",
                boxShadow: "none"
              }}
            >
              {currentOption.icon}
              {currentOption.label}
            </button>

            {dropdownOpen && (
              <div
                className="permission-dropdown"
                style={{
                  position: "absolute",
                  bottom: "100%",
                  left: "0",
                  marginBottom: "8px",
                  zIndex: 1000,
                  width: "380px",
                  background: "var(--panel)",
                  border: "1px solid var(--line)",
                  borderRadius: "8px",
                  boxShadow: "0 4px 20px rgba(0, 0, 0, 0.3)",
                  padding: "16px",
                  display: "flex",
                  flexDirection: "column",
                  gap: "12px"
                }}
              >
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center"
                  }}
                >
                  <span
                    style={{
                      fontSize: "12px",
                      color: "var(--text-soft)",
                      fontWeight: 500
                    }}
                  >
                    {isZh ? "如何授权 Yode 的操作？" : "How should Yode actions be approved?"}
                  </span>
                  <a
                    href="#"
                    onClick={(e) => e.preventDefault()}
                    style={{
                      fontSize: "12px",
                      color: "var(--text-soft)",
                      textDecoration: "underline"
                    }}
                  >
                    {isZh ? "了解更多" : "Learn more"}
                  </a>
                </div>

                <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                  {OPTIONS.map((option) => {
                    const isSelected = option.key.toLowerCase() === currentOption.key.toLowerCase();
                    return (
                      <button
                        key={option.key}
                        type="button"
                        onClick={() => {
                          onPermissionModeChange(option.key);
                          setDropdownOpen(false);
                        }}
                        style={{
                          display: "flex",
                          alignItems: "flex-start",
                          gap: "12px",
                          width: "100%",
                          padding: "10px",
                          background: isSelected ? "rgba(255, 255, 255, 0.05)" : "transparent",
                          border: "none",
                          borderRadius: "6px",
                          textAlign: "left",
                          cursor: "pointer",
                          transition: "background 0.2s",
                          outline: "none",
                          boxShadow: "none"
                        }}
                        className="dropdown-option-btn"
                      >
                        <div style={{ marginTop: "2px", color: isSelected ? "var(--accent)" : "var(--text-soft)" }}>
                          {option.icon}
                        </div>
                        <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: "2px" }}>
                          <span style={{ fontSize: "13px", fontWeight: 500, color: "var(--text)" }}>
                            {option.label}
                          </span>
                          <span style={{ fontSize: "11px", color: "var(--text-soft)", lineHeight: "1.4" }}>
                            {option.description}
                          </span>
                        </div>
                        {isSelected && (
                          <Check size={14} style={{ color: "var(--accent)", alignSelf: "center" }} />
                        )}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}
          </div>

          <div ref={modelDropdownRef} style={{ display: "inline-block", position: "relative" }}>
            <button
              className="mode-chip"
              type="button"
              onClick={() => setModelDropdownOpen(!modelDropdownOpen)}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: "6px",
                cursor: "pointer",
                outline: "none",
                boxShadow: "none"
              }}
            >
              <TopbarProviderIcon id={currentProvider} />
              <span>{currentModel || (isZh ? "选择模型" : "Select model")}</span>
              <ChevronDown size={11} style={{ opacity: 0.7, transform: modelDropdownOpen ? "rotate(180deg)" : "none", transition: "transform 150ms" }} />
            </button>

            {modelDropdownOpen && (
              <div className="context-dropdown model-dropdown">
                {modelOptions.map((model: string) => {
                  const selected = model === currentModel;
                  return (
                    <button
                      key={model}
                      type="button"
                      className={`context-option ${selected ? "selected" : ""}`}
                      onClick={() => {
                        onModelChange(model);
                        setModelDropdownOpen(false);
                      }}
                    >
                      <TopbarProviderIcon id={currentProvider} />
                      <span>{model}</span>
                      {selected ? <Check size={14} style={{ color: "var(--accent)" }} /> : <span />}
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </div>
        <div className="composer-actions">
          {isProcessing ? (
            <button 
              className="send-button stop-button" 
              onClick={onCancelMessage} 
              type="button" 
              title={isZh ? "终止" : "Stop"} 
              style={{ 
                background: "transparent", 
                border: "none", 
                color: "var(--error)", 
                outline: "none", 
                boxShadow: "none",
                display: "inline-grid",
                placeItems: "center",
                transition: "color 0.15s ease",
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = "transparent";
                e.currentTarget.style.color = "color-mix(in oklch, var(--error), var(--text) 20%)";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = "transparent";
                e.currentTarget.style.color = "var(--error)";
              }}
            >
              <Square size={13} fill="currentColor" style={{ borderRadius: "1px" }} />
            </button>
          ) : (
            <button className="send-button" onClick={onSendMessage} type="button" title={isZh ? "发送" : "Send"} style={{ outline: "none", boxShadow: "none" }}>
              <Send size={17} />
            </button>
          )}
        </div>
      </div>
    </footer>
  );
}

function desktopEventToTimelineItem(
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

  if (kind === "turn_started") {
    return {
      id: turnId ? `reasoning-${turnId}` : `event-${Date.now()}-${Math.random()}`,
      kind: "reasoning",
      title: title || "思考中",
      body: body || "",
      meta: "running",
      createdAt: Date.now()
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
      turnId
    };
  }

  if (kind === "ask_user") {
    return {
      id: eventId ? `ask-${turnId}-${eventId}` : `event-${Date.now()}-${Math.random()}`,
      kind: "assistant",
      title,
      body,
      meta: "waiting for input"
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
      meta
    };
  }

  if (kind === "assistant_reasoning_delta") {
    return {
      id: `event-${Date.now()}-${Math.random()}`,
      kind: "reasoning",
      title,
      body: "",
      meta
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
    kind === "context_compaction_started"
  ) {
    return {
      id: `event-${Date.now()}-${Math.random()}`,
      kind: "reasoning",
      title,
      body,
      meta: status === "running" ? "running" : meta
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

function applyDesktopEventToTimelineItems(
  items: TimelineItem[],
  payload: any,
  eventKind?: string
): TimelineItem[] {
  const outer = payload && typeof payload === "object" && "payload" in payload ? payload : null;
  const inner = outer ? outer.payload : payload;
  const kind = eventKind ?? stringValue(outer?.kind) ?? stringValue(inner?.kind) ?? stringValue(inner?.type);
  const body = stringValue(inner?.body) ?? "";
  const reasoning = stringValue(inner?.reasoning) ?? "";
  const turnId = stringValue(outer?.turnId);
  const assistantId = turnId ? `assistant-${turnId}` : undefined;
  const reasoningId = turnId ? `reasoning-${turnId}` : undefined;
  const eventId = stringValue(inner?.id);
  const status = stringValue(inner?.status);
  const hasToolCalls = Boolean(inner?.hasToolCalls);

  if (kind === "tool_started" || kind === "tool_progress" || kind === "tool_result" || kind === "subagent_started" || kind === "subagent_completed") {
    const nextItem = desktopEventToTimelineItem(payload, eventKind);
    const existingIndex = items.findIndex((item) => item.id === nextItem.id);
    if (existingIndex >= 0 && nextItem.kind === "tool") {
      return items.map((item, index) =>
        index === existingIndex && item.kind === "tool"
          ? {
              ...item,
              title: nextItem.title || item.title,
              body: nextItem.body || item.body,
              status: nextItem.status,
              meta: nextItem.meta ?? item.meta
            }
          : item
      );
    }
    return [...items, nextItem];
  }

  if (kind === "turn_started") {
    const thinkingId = turnId ? `reasoning-${turnId}` : undefined;
    if (
      items.some((item) =>
        thinkingId
          ? item.id === thinkingId
          : item.kind === "reasoning" && item.meta === "running"
      )
    ) {
      return items;
    }
    return [...items, desktopEventToTimelineItem(payload, eventKind)];
  }

  if (kind === "assistant_text_delta") {
    const existingIndex = assistantId
      ? items.findIndex((item) => item.id === assistantId)
      : items.findIndex((item) => item.kind === "assistant" && item.meta !== "stream complete");
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
        id: assistantId ?? `event-${Date.now()}-${Math.random()}`,
        kind: "assistant",
        title: "Yode",
        body,
        meta: "streaming"
      }
    ];
  }

  if (kind === "assistant_text_complete") {
    const existingIndex = assistantId
      ? items.findIndex((item) => item.id === assistantId)
      : items.findIndex((item) => item.kind === "assistant" && item.meta !== "stream complete");
    if (existingIndex >= 0) {
      return items.map((item, index) =>
        index === existingIndex && item.kind === "assistant"
          ? { ...item, body: body || item.body, meta: "stream complete" }
          : item
      );
    }
    if (body) {
      return [
        ...items,
        {
          id: assistantId ?? `event-${Date.now()}-${Math.random()}`,
          kind: "assistant",
          title: "Yode",
          body,
          meta: "stream complete"
        }
      ];
    }
    return items;
  }

  if (kind === "assistant_reasoning_delta") {
    const existingIndex = reasoningId
      ? items.findIndex((item) => item.id === reasoningId)
      : items.findIndex((item) => item.kind === "reasoning" && item.meta === "running");
    if (existingIndex >= 0) {
      return items.map((item, index) =>
        index === existingIndex && item.kind === "reasoning"
          ? item
          : item
      );
    }
    return [
      ...items,
      {
        id: reasoningId ?? `event-${Date.now()}-${Math.random()}`,
        kind: "reasoning",
        title: "思考中...",
        body: "",
        meta: "running",
        createdAt: Date.now()
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
    const existingIndex = reasoningId
      ? items.findIndex((item) => item.id === reasoningId)
      : items.findIndex((item) => item.kind === "reasoning" && item.meta === "running");
    if (existingIndex >= 0) {
      return items.map((item, index) => {
        if (index === existingIndex && item.kind === "reasoning") {
          const start = (item as any).createdAt || Date.now();
          const duration = Math.max(1, Math.round((Date.now() - start) / 1000));
          return { 
            ...item, 
            body: "", 
            meta: "complete",
            title: `已思考 ${duration} 秒`
          };
        }
        return item;
      });
    }
    return [
      ...items,
      {
        id: `event-${Date.now()}-${Math.random()}`,
        kind: "reasoning",
        title: "已思考",
        body: "",
        meta: "complete"
      }
    ];
  }

  if (kind === "turn_completed") {
    let hasAssistantForTurn = false;
    let hasReasoningForTurn = false;
    const settledItems = items.map((item, index) => {
      if (item.kind === "reasoning" && (item.meta === "running" || item.id === reasoningId)) {
        hasReasoningForTurn = true;
        return { ...item, body: "", meta: "complete" };
      }
      if (item.kind === "tool" && item.status === "running") {
        return { ...item, status: "success" as const };
      }
      if (item.kind === "assistant" && (item.id === assistantId || index === items.length - 1)) {
        hasAssistantForTurn = true;
        return { ...item, body: body || item.body, meta: hasToolCalls ? "intermediate" : "stream complete" };
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
        body: "",
        meta: "complete"
      });
    }
    if (body && !hasAssistantForTurn) {
      fallbackItems.push({
        id: `event-${Date.now()}-${Math.random()}`,
        kind: "assistant",
        title: "Yode",
        body,
        meta: hasToolCalls ? "intermediate" : "stream complete"
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
            body: newBody,
            meta: "stream complete"
          };
        }
        return item;
      });
    }

    return [
      ...settledItems,
      {
        id: errorId,
        kind: "assistant",
        title: "错误",
        body: errorMessage,
        meta: "stream complete"
      }
    ];
  }

  return [...items, desktopEventToTimelineItem(payload, eventKind)];
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function mergeStreamingText(current: string, incoming: string): string {
  if (!incoming) return current;
  if (!current || incoming.startsWith(current)) return incoming;
  return `${current}${incoming}`;
}

function RunInspector({
  isProcessing,
  permissionMode,
  timelineItems
}: {
  isProcessing: boolean;
  permissionMode: string;
  timelineItems: TimelineItem[];
}) {
  const toolItems = timelineItems.filter((item) => item.kind === "tool");
  const completedToolItems = toolItems.filter((item) => item.status !== "running");
  return (
    <aside className="run-inspector" aria-label="运行详情">
      <div className="inspector-head">
        <span>TURN</span>
        <strong>{timelineItems.length} events</strong>
      </div>
      <div className="inspector-section">
        <div className="metric-row">
          <span>状态</span>
          <strong className={isProcessing ? "state-live" : ""}>{isProcessing ? "streaming" : "idle"}</strong>
        </div>
        <div className="metric-row">
          <span>权限</span>
          <strong>{permissionMode}</strong>
        </div>
        <div className="metric-row">
          <span>上下文</span>
          <strong>{timelineItems.length > 0 ? "active" : "empty"}</strong>
        </div>
        <div className="metric-row">
          <span>工具</span>
          <strong>{completedToolItems.length} / {toolItems.length}</strong>
        </div>
      </div>
      <div className="inspector-section">
        <span className="inspector-label">NEXT</span>
        <p>{isProcessing ? "正在等待模型或工具返回。" : "选择会话或发送消息继续。"}</p>
      </div>
    </aside>
  );
}
