import type { TimelineItem } from "./desktopTypes";
import { parseJsonObject, recordFromUnknown } from "./jsonUtils";

type ActivityToolItem = Extract<TimelineItem, { kind: "tool" }>;
type CountedActivityToolItem = ActivityToolItem & { count?: number };
type ToolDetailsItem = {
  tool: string;
  body?: string;
  title?: string;
  metadata?: unknown;
};

export function isRuntimeNoticeText(text?: string) {
  if (!text) return false;
  return /limit instead of re-reading|budget notice|budget warning|checkpoint:|tool calls used|工具调用提醒|summariz(?:e|ing) current findings|most efficient next step|aggregate budget exceeded|context overflow|full result .* omitted|content omitted to prevent context overflow/i.test(text);
}

export function displayToolName(tool?: string) {
  const name = (tool || "").trim();
  if (!name) return "工具";
  if (name === "project_map") return "项目结构";
  if (name === "glob") return "文件匹配";
  if (name === "grep" || name === "rg") return "内容搜索";
  if (name === "ls") return "目录列表";
  if (name === "tauri command") return "桌面命令";
  if (name === "view_file") return "查看文件";
  if (name === "write_file") return "写入文件";
  if (name === "replace_file_content") return "编辑文件";
  if (name === "multi_replace_file_content") return "多处编辑文件";
  if (name === "write_to_file") return "创建文件";
  if (name === "run_command") return "运行命令";
  if (name === "grep_search") return "搜索内容";
  if (name === "list_dir") return "列出目录";
  if (name === "ask_permission") return "申请权限";
  if (name === "ask_question") return "提出问题";
  if (name === "search_web") return "网络搜索";
  if (name === "read_url_content") return "读取网页";
  if (name === "define_subagent") return "定义子代理";
  if (name === "invoke_subagent") return "启动子代理";
  if (name === "manage_subagents") return "管理子代理";
  if (name === "manage_task") return "管理任务";
  if (name === "todo") return "任务列表";
  if (name === "update_plan") return "更新计划";
  if (name === "write_stdin") return "写入输入";
  if (name === "schedule") return "计划任务";
  if (name === "enter_plan_mode") return "进入计划模式";
  if (name === "exit_plan_mode") return "退出计划模式";
  if (name === "verify_plan_execution") return "检查计划执行";
  return name;
}

export function looksLikeShellCommand(text: string) {
  const clean = text.trim();
  if (!clean || clean.length > 160) return false;
  if (/[\u4e00-\u9fff]/.test(clean)) return false;
  return /^(cargo|pnpm|npm|yarn|bun|git|rg|grep|find|ls|cat|sed|awk|bash|zsh|sh|python|node|deno|make|cmake|go|rustc|tsc|vite)\b/.test(clean) ||
    /(\s&&\s|\s\|\s|\s;\s|^\.\/|^\w+=\S+\s+\w+)/.test(clean);
}

export function shouldHideActivityItem(item: TimelineItem) {
  return (
    ("title" in item && isRuntimeNoticeText(item.title)) ||
    ("body" in item && isRuntimeNoticeText(item.body)) ||
    ("result" in item && isRuntimeNoticeText(item.result))
  );
}

export function isThinkingStatusTitle(title?: string) {
  return /思考|thinking|thought/i.test(title || "");
}

export type ActivityKind = "read" | "search" | "run" | "edit" | "mcp" | "image" | "other";

export type ActivityDescriptor = {
  kind: ActivityKind;
  label: string;
  target: string;
  command?: string;
  filename?: string;
  lineRange?: string;
  diff?: string;
  diffPreview?: string;
  modifiedFiles: string[];
};

function stringField(object: unknown, keys: string[]) {
  const record = recordFromUnknown(object);
  if (!record) return "";
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return "";
}

function baseName(path: string) {
  const clean = path.trim();
  if (!clean) return "";
  return clean.substring(Math.max(clean.lastIndexOf("/"), clean.lastIndexOf("\\")) + 1);
}

function normalizeActivityKind(value: string): ActivityKind {
  const kind = value.toLowerCase();
  if (kind === "read" || kind === "explore" || kind === "list" || kind === "view") return "read";
  if (kind === "search" || kind === "web") return "search";
  if (kind === "run" || kind === "command" || kind === "shell") return "run";
  if (kind === "edit" || kind === "write" || kind === "patch") return "edit";
  if (kind === "mcp") return "mcp";
  if (kind === "image") return "image";
  return "other";
}

function inferActivityKind(tool: string, title: string, parsed: ReturnType<typeof parseToolDetails>, metadata: unknown): ActivityKind {
  const metadataRecord = recordFromUnknown(metadata);
  const activityKind = stringField(metadataRecord?.activity, ["kind", "type"]);
  if (activityKind) return normalizeActivityKind(activityKind);

  const name = `${tool} ${title}`.toLowerCase();
  if (parsed.modifiedFiles.length > 0 || parsed.diff || metadataRecord?.diff_preview) return "edit";
  if (/write|edit|replace|patch|modify|create|写入|修改|编辑|替换/.test(name)) return "edit";
  if (/run|command|bash|shell|execute|cmd|sh|运行|执行/.test(name)) return "run";
  if (/search|grep|url|web|网页|搜索/.test(name)) return "search";
  if (/read|view|list|grep|map|glob|ls|find|locate|project_map|resource|探索|查看|读取/.test(name)) return "read";
  return "other";
}

export function parseToolDetails(item: ToolDetailsItem) {
  let filename = "";
  let lineRange = "";
  let diff = "";
  let command = "";
  let diffPreview = "";
  let modifiedFiles: string[] = [];

  const body = (item.body || "").trim();
  const title = (item.title || "").trim();
  const metadata = recordFromUnknown(item.metadata);

  if (isRuntimeNoticeText(body) || isRuntimeNoticeText(title)) {
    return { filename, lineRange, diff, command, diffPreview, modifiedFiles };
  }

  if (metadata) {
    const rawPath = metadata.file_path || metadata.path || metadata.TargetFile || metadata.AbsolutePath || metadata.Path || metadata.Target || metadata.SearchPath || metadata.TargetContentFile;
    if (rawPath && typeof rawPath === "string") {
      filename = rawPath.substring(Math.max(rawPath.lastIndexOf('/'), rawPath.lastIndexOf('\\')) + 1);
    }

    const start = metadata.start_line ?? metadata.startLine ?? metadata.StartLine;
    const end = metadata.end_line ?? metadata.endLine ?? metadata.EndLine;
    if (start !== undefined && end !== undefined) {
      lineRange = `#L${start}-${end}`;
    } else if (start !== undefined) {
      lineRange = `#L${start}`;
    }

    if (Array.isArray(metadata.modified_files)) {
      modifiedFiles = metadata.modified_files.filter(
        (value: unknown): value is string => typeof value === "string" && value.trim().length > 0
      );
      if (!filename && modifiedFiles.length === 1) {
        filename = modifiedFiles[0];
      }
    }

    const preview = recordFromUnknown(metadata.diff_preview);
    if (preview) {
      const removed = Array.isArray(preview.removed) ? preview.removed.map(String) : [];
      const added = Array.isArray(preview.added) ? preview.added.map(String) : [];
      const moreRemoved = Number(preview.more_removed || 0);
      const moreAdded = Number(preview.more_added || 0);
      const removedCount = removed.length + (Number.isFinite(moreRemoved) ? moreRemoved : 0);
      const addedCount = added.length + (Number.isFinite(moreAdded) ? moreAdded : 0);
      diff = `+${addedCount} -${removedCount}`;
      diffPreview = [
        ...removed.map((line: string) => `-${line}`),
        ...added.map((line: string) => `+${line}`)
      ].join("\n");
      if (moreRemoved > 0 || moreAdded > 0) {
        diffPreview += `\n... 还有 ${moreRemoved + moreAdded} 行未显示`;
      }
    }
  }

  const parsed = parseJsonObject(body);
  if (parsed) {
      const rawPath = parsed.file_path || parsed.path || parsed.TargetFile || parsed.AbsolutePath || parsed.Path || parsed.Target || parsed.SearchPath || parsed.TargetContentFile;
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

      const parsedCommand = parsed.CommandLine || parsed.command || parsed.cmd || parsed.shell_command;
      if (typeof parsedCommand === "string" && parsedCommand.trim()) {
        command = parsedCommand.trim();
      }

      if (item.tool?.includes("replace") || item.tool?.includes("write") || item.tool?.includes("edit")) {
        const target = typeof parsed.TargetContent === "string"
          ? parsed.TargetContent
          : typeof parsed.targetContent === "string"
            ? parsed.targetContent
            : "";
        const replacement = [
          parsed.ReplacementContent,
          parsed.replacementContent,
          parsed.CodeContent,
          parsed.codeContent,
          parsed.content,
          parsed.file_content,
        ].find((value): value is string => typeof value === "string") || "";
        if (target || replacement) {
          const targetLines = target ? target.split("\n").length : 0;
          const replacementLines = replacement ? replacement.split("\n").length : 0;
          diff = `+${replacementLines} -${targetLines}`;
        }
      }
  } else {
    const pathMatch = body.match(/"(?:file_path|path|AbsolutePath|TargetFile|Path|SearchPath)"\s*:\s*"([^"]+)"/);
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
      const replacementMatch = body.match(/"(?:ReplacementContent|replacementContent|CodeContent|codeContent|content|file_content)"\s*:\s*"([\s\S]*?)"/);
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
    const parts = title.split(/[\s/\\]+/);
    const lastPart = parts[parts.length - 1];
    if (lastPart && lastPart.includes(".") && !lastPart.includes("]")) {
      filename = lastPart;
    }
  }

  return { filename, lineRange, diff, command, diffPreview, modifiedFiles };
}

export function getActivityDescriptor(item: ToolDetailsItem): ActivityDescriptor {
  const parsed = parseToolDetails(item);
  const metadata = recordFromUnknown(item.metadata);
  const activity = recordFromUnknown(metadata?.activity);
  const parsedBody = parseJsonObject(item.body);

  const command = stringField(activity, ["command"]) || parsed.command;
  const rawFile =
    stringField(activity, ["file_path", "path"]) ||
    parsed.filename ||
    stringField(metadata, ["file_path", "path", "uri"]);
  const filename = rawFile ? baseName(rawFile) : "";
  const kind = inferActivityKind(item.tool || "", item.title || "", parsed, metadata);
  const searchTarget =
    kind === "search"
      ? stringField(activity, ["pattern", "query"]) ||
        stringField(metadata, ["pattern", "query"]) ||
        stringField(parsedBody, ["pattern", "query", "SearchPattern", "Pattern"])
      : "";
  const target =
    stringField(activity, ["target", "label"]) ||
    searchTarget ||
    command ||
    filename ||
    stringField(metadata, ["pattern", "uri", "server", "name"]) ||
    displayToolName(item.tool);
  const label = stringField(activity, ["label"]) || target;

  return {
    kind,
    label,
    target,
    command: command || undefined,
    filename: filename || undefined,
    lineRange: parsed.lineRange || undefined,
    diff: parsed.diff || undefined,
    diffPreview: parsed.diffPreview || undefined,
    modifiedFiles: parsed.modifiedFiles,
  };
}

export function summarizeActivityItems(items: TimelineItem[]) {
  const summarized: TimelineItem[] = [];
  const seen = new Map<string, CountedActivityToolItem>();

  for (const item of expandBatchActivityItems(items)) {
    if (shouldHideActivityItem(item)) continue;

    if (item.kind !== "tool") {
      summarized.push(item);
      continue;
    }

    const descriptor = getActivityDescriptor(item);
    const key = [
      item.kind,
      item.tool || "",
      descriptor.filename || descriptor.command || descriptor.target || "",
    ].join(":");

    const existing = seen.get(key);
    if (existing) {
      existing.count = (existing.count || 1) + 1;
      if (item.status === "running") existing.status = "running";
      if (!existing.result && item.result) existing.result = item.result;
      continue;
    }

    const next: CountedActivityToolItem = { ...item, count: 1 };
    seen.set(key, next);
    summarized.push(next);
  }

  return summarized;
}

function expandBatchActivityItems(items: TimelineItem[]) {
  const expanded: TimelineItem[] = [];

  for (const item of items) {
    if (item?.kind !== "tool" || item.tool !== "batch") {
      expanded.push(item);
      continue;
    }

    const parsed = parseJsonObject(item.body);
    const invocations = Array.isArray(parsed?.invocations) ? parsed.invocations : [];
    if (invocations.length === 0) {
      expanded.push(item);
      continue;
    }

    invocations.forEach((rawInvocation, index) => {
      const invocation = recordFromUnknown(rawInvocation);
      const toolName = typeof invocation?.tool_name === "string" ? invocation.tool_name : "tool";
      const params = recordFromUnknown(invocation?.params) ?? {};
      expanded.push({
        ...item,
        id: `${item.id}-batch-${index}`,
        tool: toolName,
        title: displayToolName(toolName),
        body: JSON.stringify(params),
        metadata: undefined,
      });
    });
  }

  return expanded;
}

export function activityItemSummary(item: TimelineItem) {
  if (item.kind !== "tool") return "";
  const descriptor = getActivityDescriptor(item);
  if (descriptor.kind === "search" && descriptor.target) return descriptor.target;
  if (descriptor.kind === "run" && descriptor.command) return descriptor.command;
  if (descriptor.filename) return descriptor.filename;
  if (descriptor.command) return descriptor.command;
  if (descriptor.target) return descriptor.target;
  return displayToolName(item.tool);
}

function compactTarget(text: string) {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (normalized.length <= 58) return normalized;
  return `${normalized.slice(0, 55)}...`;
}

function uniqueTargets(items: CountedActivityToolItem[], max = 2) {
  const labels: string[] = [];
  let total = 0;

  for (const item of items) {
    const label = compactTarget(activityItemSummary(item));
    if (!label) continue;
    total += item.count && Number.isFinite(item.count) ? Number(item.count) : 1;
    if (!labels.includes(label) && labels.length < max) labels.push(label);
  }

  return { labels, total: total || items.length };
}

function groupPhrase(items: CountedActivityToolItem[], kind: ActivityKind, appLang: string) {
  const isZh = appLang === "zh";
  const { labels, total } = uniqueTargets(items);
  if (labels.length === 0) return "";

  const suffix = total > labels.length
    ? isZh
      ? ` 等 ${total} 项`
      : ` and ${total - labels.length} more`
    : "";
  const targets = isZh ? `${labels.join("、")}${suffix}` : `${labels.join(", ")}${suffix}`;

  if (isZh) {
    if (kind === "read") return `查看 ${targets}`;
    if (kind === "search") return `搜索 ${targets}`;
    if (kind === "run") return `运行 ${targets}`;
    if (kind === "edit") return `修改 ${targets}`;
    return `执行 ${targets}`;
  }

  if (kind === "read") return `read ${targets}`;
  if (kind === "search") return `search ${targets}`;
  if (kind === "run") return `run ${targets}`;
  if (kind === "edit") return `edit ${targets}`;
  return `execute ${targets}`;
}

export function activityGroupPreview(items: TimelineItem[], appLang: string) {
  const isZh = appLang === "zh";
  const tools = items.filter((item): item is CountedActivityToolItem => item.kind === "tool");
  const phrases = (["read", "search", "run", "edit", "mcp", "image", "other"] as ActivityKind[])
    .map((kind) => groupPhrase(
      tools.filter((item) => getActivityDescriptor(item).kind === kind),
      kind,
      appLang
    ))
    .filter(Boolean);

  if (phrases.length === 0) {
    return isZh ? "点击展开查看活动明细" : "Expand to view activity details";
  }

  const visiblePhrases = phrases.slice(0, 3);
  const suffix = phrases.length > visiblePhrases.length
    ? isZh
      ? `，还有 ${phrases.length - visiblePhrases.length} 类操作`
      : `, plus ${phrases.length - visiblePhrases.length} more categories`
    : "";

  return `${visiblePhrases.join(isZh ? "，" : "; ")}${suffix}`;
}
