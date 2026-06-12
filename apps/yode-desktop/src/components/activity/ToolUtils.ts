export function isRuntimeNoticeText(text?: string) {
  if (!text) return false;
  return /limit instead of re-reading|budget notice|budget warning|checkpoint:|tool calls used|工具调用提醒|summariz(?:e|ing) current findings|most efficient next step/i.test(text);
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
  if (name === "schedule") return "计划任务";
  return name;
}

export function looksLikeShellCommand(text: string) {
  const clean = text.trim();
  if (!clean || clean.length > 160) return false;
  if (/[\u4e00-\u9fff]/.test(clean)) return false;
  return /^(cargo|pnpm|npm|yarn|bun|git|rg|grep|find|ls|cat|sed|awk|bash|zsh|sh|python|node|deno|make|cmake|go|rustc|tsc|vite)\b/.test(clean) ||
    /(\s&&\s|\s\|\s|\s;\s|^\.\/|^\w+=\S+\s+\w+)/.test(clean);
}

export function shouldHideActivityItem(item: any) {
  return isRuntimeNoticeText(item?.title) || isRuntimeNoticeText(item?.body) || isRuntimeNoticeText(item?.result);
}

export function isThinkingStatusTitle(title?: string) {
  return /思考|thinking|thought/i.test(title || "");
}

export function parseToolDetails(item: { tool: string; body: string; title: string; metadata?: any }) {
  let filename = "";
  let lineRange = "";
  let diff = "";
  let command = "";
  let diffPreview = "";
  let modifiedFiles: string[] = [];

  const body = (item.body || "").trim();
  const title = (item.title || "").trim();
  const metadata = item.metadata && typeof item.metadata === "object" ? item.metadata : null;

  if (isRuntimeNoticeText(body) || isRuntimeNoticeText(title)) {
    return { filename, lineRange, diff, command, diffPreview, modifiedFiles };
  }

  if (metadata) {
    const rawPath = metadata.file_path || metadata.TargetFile || metadata.AbsolutePath || metadata.Path || metadata.Target || metadata.SearchPath || metadata.TargetContentFile;
    if (rawPath && typeof rawPath === "string") {
      filename = rawPath.substring(Math.max(rawPath.lastIndexOf('/'), rawPath.lastIndexOf('\\')) + 1);
    }

    if (Array.isArray(metadata.modified_files)) {
      modifiedFiles = metadata.modified_files.filter(
        (value: unknown): value is string => typeof value === "string" && value.trim().length > 0
      );
      if (!filename && modifiedFiles.length === 1) {
        filename = modifiedFiles[0];
      }
    }

    const preview = metadata.diff_preview;
    if (preview && typeof preview === "object") {
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

  return { filename, lineRange, diff, command, diffPreview, modifiedFiles };
}

export function summarizeActivityItems(items: any[]) {
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

export function activityItemSummary(item: any) {
  if (item.kind !== "tool") return "";
  const parsed = parseToolDetails(item);
  if (parsed.filename) return parsed.filename;
  if (parsed.command) return parsed.command;
  return displayToolName(item.tool);
}

export function activityGroupPreview(items: any[], appLang: string) {
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
