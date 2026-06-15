import React, { useEffect, useMemo, useState } from "react";
import { ChevronDown, ChevronRight, CircleDot, Copy, Pencil, Check } from "lucide-react";
import { TimelineItem } from "../../lib/mock";
import { ActivityLeafNode } from "./ActivityLeafNode";
import { getFileIcon } from "../FileIcon";
import {
  parseToolDetails,
  displayToolName,
  summarizeActivityItems,
  getActivityDescriptor
} from "./ToolUtils";

function classifyActivityTool(item: any) {
  return getActivityDescriptor(item).kind;
}

function noun(count: number, zhSingular: string, enSingular: string, enPlural: string, isZh: boolean) {
  if (isZh) return `${count} ${zhSingular}`;
  return `${count} ${count === 1 ? enSingular : enPlural}`;
}

function buildActivityGroupLabel(items: any[], appLang: string, isRunning: boolean) {
  const isZh = appLang === "zh";
  const tools = items.filter((item) => item.kind === "tool");
  const exploreTools = tools.filter((item) => classifyActivityTool(item) === "read");
  const searchTools = tools.filter((item) => classifyActivityTool(item) === "search");
  const runTools = tools.filter((item) => classifyActivityTool(item) === "run");
  const editTools = tools.filter((item) => classifyActivityTool(item) === "edit");
  const otherTools = tools.filter((item) => !["read", "search", "run", "edit"].includes(classifyActivityTool(item)));
  const parts: string[] = [];

  if (exploreTools.length > 0) {
    const files = new Set<string>();
    exploreTools.forEach((item) => {
      const descriptor = getActivityDescriptor(item);
      if (descriptor.filename) files.add(descriptor.filename);
    });
    const count = files.size || exploreTools.length;
    parts.push(isZh
      ? `${isRunning ? "正在查看" : "已查看"} ${noun(count, files.size > 0 ? "个文件" : "项", "file", "files", true)}`
      : `${isRunning ? "Exploring" : "Explored"} ${noun(count, "", "file", "files", false)}`);
  }

  if (searchTools.length > 0) {
    parts.push(isZh
      ? `${isRunning ? "正在搜索" : "已搜索"} ${noun(searchTools.length, "次", "web search", "web searches", true)}`
      : `${isRunning ? "Searching" : "Searched"} ${noun(searchTools.length, "", "web search", "web searches", false)}`);
  }

  if (runTools.length > 0) {
    parts.push(isZh
      ? `${isRunning ? "正在运行" : "已运行"} ${noun(runTools.length, "条命令", "command", "commands", true)}`
      : `${isRunning ? "Running" : "Ran"} ${noun(runTools.length, "", "command", "commands", false)}`);
  }

  if (editTools.length > 0) {
    const files = new Set<string>();
    editTools.forEach((item) => {
      const descriptor = getActivityDescriptor(item);
      if (descriptor.filename) files.add(descriptor.filename);
    });
    const count = files.size || editTools.length;
    parts.push(isZh
      ? `${isRunning ? "正在修改" : "已修改"} ${noun(count, files.size > 0 ? "个文件" : "项", "file", "files", true)}`
      : `${isRunning ? "Editing" : "Edited"} ${noun(count, "", "file", "files", false)}`);
  }

  if (otherTools.length > 0) {
    parts.push(isZh
      ? `${isRunning ? "正在执行" : "已执行"} ${noun(otherTools.length, "个操作", "action", "actions", true)}`
      : `${isRunning ? "Executing" : "Executed"} ${noun(otherTools.length, "", "action", "actions", false)}`);
  }

  return parts.join(isZh ? "" : ", ");
}

export function ActivityGroupNode({ group, appLang, isTurnActive }: { group: any; appLang: string; isTurnActive?: boolean }) {
  const isZh = appLang === "zh";
  const visibleItems = useMemo(() => summarizeActivityItems(group.items || []), [group.items]);
  const isRunning = group.status === "running";
  const shouldAutoExpand = isRunning && visibleItems.length > 0 && visibleItems.length <= 4;
  const [isExpanded, setIsExpanded] = useState(shouldAutoExpand);
  const [hasManuallyToggled, setHasManuallyToggled] = useState(false);

  useEffect(() => {
    setHasManuallyToggled(false);
  }, [group.id]);

  useEffect(() => {
    if (hasManuallyToggled) return;

    if (isRunning) {
      setIsExpanded(true);
    } else {
      setIsExpanded(false);
    }
  }, [group.id, isRunning, isTurnActive, hasManuallyToggled]);

  if (visibleItems.length === 0 && !isRunning) return null;

  const count = visibleItems.filter((item: any) => item.kind === "tool").length || 1;
  const displayedItems = isExpanded && visibleItems.length > 8
    ? [...visibleItems.slice(0, 4), ...visibleItems.slice(-3)]
    : visibleItems;
  const hiddenCount = isExpanded && visibleItems.length > displayedItems.length
    ? visibleItems.length - displayedItems.length
    : 0;
  
  const label = buildActivityGroupLabel(visibleItems, appLang, isRunning) ||
    (isZh ? (isRunning ? "正在执行..." : `已执行 ${count} 个操作`) : (isRunning ? "Working..." : `Executed ${count} action${count > 1 ? "s" : ""}`));

  return (
    <div className="activity-group-node">
      <div 
        onClick={() => {
          setIsExpanded(!isExpanded);
          setHasManuallyToggled(true);
        }}
        className="activity-group-trigger"
      >
        <span>{label}</span>
        {isExpanded ? <ChevronDown size={12} className="activity-chevron strong" /> : <ChevronRight size={12} className="activity-chevron strong" />}
      </div>

      {isExpanded && (
        <div className="activity-group-items">
          {displayedItems.map((item: any, idx: number) => (
            <ActivityLeafNode key={item.id || idx} item={item} appLang={appLang} />
          ))}
          {hiddenCount > 0 && (
            <div className="activity-hidden-count">
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
                const descriptor = getActivityDescriptor(runningItem);
                if (descriptor.kind === "run") {
                  statusText = descriptor.command
                    ? (isZh ? `正在运行命令 ${descriptor.command}...` : `Running command ${descriptor.command}...`)
                    : (isZh ? "正在运行命令..." : "Running command...");
                } else if (descriptor.kind === "read" && descriptor.filename) {
                  statusText = isZh ? `正在读取 ${descriptor.filename}...` : `Reading ${descriptor.filename}...`;
                } else if (descriptor.kind === "search") {
                  statusText = isZh ? `正在搜索 ${descriptor.target}...` : `Searching ${descriptor.target}...`;
                } else if (descriptor.kind === "edit" && descriptor.filename) {
                  statusText = isZh ? `正在修改 ${descriptor.filename}...` : `Editing ${descriptor.filename}...`;
                } else {
                  statusText = isZh ? `正在执行 ${displayToolName(runningItem.tool)}...` : `Executing ${displayToolName(runningItem.tool)}...`;
                }
              }
            }
            return (
              <div className="activity-running-status">
                <CircleDot size={10} className="activity-running-dot" />
                <span>{statusText}</span>
              </div>
            );
          })()}
        </div>
      )}
    </div>
  );
}

export function ActivityItemNode({ node, appLang }: { node: any; appLang: string }) {
  const isZh = appLang === "zh";
  const [isExpanded, setIsExpanded] = useState(false);

  const isRunning = node.status === "running";
  const parsed = parseToolDetails(node);
  const label = isRunning 
    ? (isZh ? "正在修改" : "Editing") 
    : (isZh ? "已修改" : "Edited");

  let addCount = "";
  let delCount = "";
  if (node.diff || parsed.diff) {
    const parts = (node.diff || parsed.diff).split(" ");
    addCount = parts[0] || "";
    delCount = parts[1] || "";
  }

  return (
    <div className="activity-group-node">
      <div 
        onClick={() => node.body && setIsExpanded(!isExpanded)}
        className={`activity-group-trigger ${node.body ? "interactive" : "static"}`}
      >
        <span>{label}</span>
        {(node.filename || parsed.filename) ? getFileIcon(node.filename || parsed.filename) : null}
        {(node.filename || parsed.filename) && (
          <span className="activity-strong">{node.filename || parsed.filename}</span>
        )}
        {addCount && <span className="diff-add activity-diff-count">{addCount}</span>}
        {delCount && <span className="diff-del activity-diff-count">{delCount}</span>}
        {node.body && (
          isExpanded ? <ChevronDown size={12} className="activity-chevron strong" /> : <ChevronRight size={12} className="activity-chevron strong" />
        )}
      </div>

      {isExpanded && (node.body || parsed.diffPreview) && (
        <div className="activity-edit-detail">
          <pre className="activity-leaf-code activity-leaf-code-result">
            {parsed.diffPreview || node.body}
          </pre>
        </div>
      )}
    </div>
  );
}

function diffCountsForEdit(item: Extract<TimelineItem, { kind: "activity_item" }>) {
  if (item.status === "blocked") {
    return { add: 0, del: 0, parsed: parseToolDetails(item as any) };
  }
  const parsed = parseToolDetails(item as any);
  const raw = item.diff || parsed.diff || "";
  const add = Number(raw.match(/\+(\d+)/)?.[1] || 0);
  const del = Number(raw.match(/-(\d+)/)?.[1] || 0);
  return { add, del, parsed };
}

function mergeEditSummaryItems(items: Array<Extract<TimelineItem, { kind: "activity_item" }>>) {
  const merged = new Map<string, Extract<TimelineItem, { kind: "activity_item" }>>();

  for (const item of items) {
    const parsed = parseToolDetails(item as any);
    const filename = item.filename || parsed.filename || "";
    const key = (item as any).callId || `${item.tool}:${filename || item.id}`;
    const existing = merged.get(key);
    if (!existing) {
      merged.set(key, item);
      continue;
    }

    const existingParsed = parseToolDetails(existing as any);
    const itemScore =
      (item.metadata ? 4 : 0) +
      (item.result ? 2 : 0) +
      (item.diff || parsed.diff ? 2 : 0) +
      (filename ? 1 : 0);
    const existingScore =
      (existing.metadata ? 4 : 0) +
      (existing.result ? 2 : 0) +
      (existing.diff || existingParsed.diff ? 2 : 0) +
      ((existing.filename || existingParsed.filename) ? 1 : 0);

    const preferred = itemScore >= existingScore ? item : existing;
    const secondary = preferred === item ? existing : item;
    const mergedItem: Extract<TimelineItem, { kind: "activity_item" }> = {
      ...preferred,
      body: preferred.body && preferred.body.trim().startsWith("{")
        ? preferred.body
        : secondary.body && secondary.body.trim().startsWith("{")
          ? secondary.body
          : preferred.body || secondary.body,
      result: secondary.result || preferred.result,
      metadata: (secondary.metadata ?? preferred.metadata) || preferred.metadata,
      status: secondary.status === "blocked" ? "blocked" : preferred.status,
    } as Extract<TimelineItem, { kind: "activity_item" }>;
    merged.set(key, mergedItem);
  }

  return Array.from(merged.values());
}

export function EditSummaryNode({ node, appLang }: { node: Extract<TimelineItem, { kind: "edit_summary" }>; appLang: string }) {
  const isZh = appLang === "zh";
  const [isExpanded, setIsExpanded] = useState(false);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const rows = mergeEditSummaryItems(node.items).map((item) => ({
    item,
    ...diffCountsForEdit(item)
  }));
  const successfulRows = rows.filter((row) => row.item.status !== "blocked");
  const blockedRows = rows.filter((row) => row.item.status === "blocked");
  const totalAdd = successfulRows.reduce((sum, row) => sum + row.add, 0);
  const totalDel = successfulRows.reduce((sum, row) => sum + row.del, 0);
  const editedLabel =
    successfulRows.length > 0
      ? isZh
        ? `已编辑 ${successfulRows.length} 个文件`
        : `Edited ${successfulRows.length} files`
      : blockedRows.length > 0
        ? isZh
          ? `未写入 ${blockedRows.length} 个文件`
          : `Write blocked for ${blockedRows.length} files`
        : isZh
          ? "已编辑 0 个文件"
          : "Edited 0 files";

  const copyDiff = async (text: string, id: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedId(id);
      window.setTimeout(() => setCopiedId((current) => (current === id ? null : current)), 1200);
    } catch (error) {
      console.error(error);
    }
  };

  return (
    <div className="edit-summary-card">
      <div className="edit-summary-head">
        <div className="edit-summary-badge">
          <Pencil size={15} />
        </div>
        <div className="edit-summary-title">
          <strong>{editedLabel}</strong>
          {successfulRows.length > 0 ? (
            <span>
              <em className="diff-add">+{totalAdd}</em>
              <em className="diff-del">-{totalDel}</em>
            </span>
          ) : null}
        </div>
        <div className="edit-summary-actions">
          <button type="button" title={isZh ? "查看改动" : "Review changes"} onClick={() => setIsExpanded((value) => !value)}>
            {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>
        </div>
      </div>

      {isExpanded ? (
        <div className="edit-summary-files">
          {rows.map(({ item, parsed, add, del }) => {
            const filename = item.filename || parsed.filename || displayToolName(item.tool);
            const isBlocked = item.status === "blocked";
            return (
              <div className="edit-summary-file" key={item.id}>
                <span className="edit-summary-file-name">
                  <span>{isBlocked ? (isZh ? "未写入" : "Blocked") : isZh ? "已编辑" : "Edited"}</span>
                  <span className="edit-summary-file-target">
                    {filename ? getFileIcon(filename) : null}
                    <span>{filename}</span>
                  </span>
                </span>
                {isBlocked ? (
                  <span className="edit-summary-file-stats">
                    <em className="diff-del">阻塞</em>
                  </span>
                ) : (
                  <span className="edit-summary-file-stats">
                    <em className="diff-add">+{add}</em>
                    <em className="diff-del">-{del}</em>
                  </span>
                )}
              </div>
            );
          })}
        </div>
      ) : null}

      {isExpanded ? (
        <div className="edit-summary-diffs">
          {rows.map(({ item, parsed, add, del }) => {
            const filename = item.filename || parsed.filename || displayToolName(item.tool);
            const preview = item.metadata?.diff_preview;
            const previewLines = (parsed.diffPreview || "")
              .split("\n")
              .filter((line) => line.startsWith("+") || line.startsWith("-"));
            const removed = Array.isArray(preview?.removed)
              ? preview.removed.map(String)
              : previewLines.filter((line) => line.startsWith("-")).map((line) => line.slice(1));
            const added = Array.isArray(preview?.added)
              ? preview.added.map(String)
              : previewLines.filter((line) => line.startsWith("+")).map((line) => line.slice(1));
            const hasStructuredDiff = item.status !== "blocked" && (removed.length > 0 || added.length > 0);
            const diffText = item.status === "blocked" ? item.result || item.body : parsed.diffPreview || item.result || item.body;
            if (!diffText && !hasStructuredDiff) return null;
            const diffCopyText = hasStructuredDiff
              ? [
                ...removed.map((line: string) => `-${line}`),
                ...added.map((line: string) => `+${line}`)
              ].join("\n")
              : diffText;
            return (
              <details className="edit-diff" key={`${item.id}-diff`}>
                <summary>
                  {filename ? getFileIcon(filename) : null}
                  <span className="edit-diff-name">{filename}</span>
                  {item.status !== "blocked" ? (
                    <span className="edit-diff-counts">
                      <em className="diff-add">+{add}</em>
                      <em className="diff-del">-{del}</em>
                    </span>
                  ) : null}
                  <button
                    type="button"
                    className="edit-diff-copy"
                    aria-label={isZh ? "复制 diff" : "Copy diff"}
                    title={isZh ? "复制 diff" : "Copy diff"}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      void copyDiff(diffCopyText, item.id);
                    }}
                  >
                    {copiedId === item.id ? <Check size={14} /> : <Copy size={14} />}
                  </button>
                </summary>
                {hasStructuredDiff ? (
                  <div className="edit-diff-lines">
                    {removed.map((line: string, index: number) => (
                      <div className="edit-diff-line removed" key={`${item.id}-removed-${index}`}>
                        <span className="edit-diff-gutter">-</span>
                        <span className="edit-diff-content">{line || "\u00a0"}</span>
                      </div>
                    ))}
                    {added.map((line: string, index: number) => (
                      <div className="edit-diff-line added" key={`${item.id}-added-${index}`}>
                        <span className="edit-diff-gutter">+</span>
                        <span className="edit-diff-content">{line || "\u00a0"}</span>
                      </div>
                    ))}
                  </div>
                ) : (
                  <pre>{diffText}</pre>
                )}
              </details>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
