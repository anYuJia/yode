import React, { useEffect, useMemo, useState } from "react";
import { ChevronDown, ChevronRight, CircleDot, FileCode2 } from "lucide-react";
import { TimelineItem } from "../../lib/mock";
import { ActivityLeafNode } from "./ActivityLeafNode";
import { getFileIcon } from "../FileIcon";
import {
  parseToolDetails,
  displayToolName,
  summarizeActivityItems,
  activityGroupPreview
} from "./ToolUtils";

export interface AgentAction {
  id: string;
  type: "explore" | "edit" | "run" | "reasoning";
  label: string;
  items: TimelineItem[];
}

export function compileTurnActions(items: TimelineItem[]): AgentAction[] {
  const actions: AgentAction[] = [];
  const toolsRun = items.filter((item): item is Extract<TimelineItem, { kind: "tool" }> => item.kind === "tool");
  
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

export function ActivityGroupNode({ group, appLang, isTurnActive }: { group: any; appLang: string; isTurnActive?: boolean }) {
  const isZh = appLang === "zh";
  const visibleItems = useMemo(() => summarizeActivityItems(group.items || []), [group.items]);
  const isRunning = group.status === "running";
  const shouldAutoExpand = visibleItems.length > 0 && visibleItems.length <= 4;
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
      fontSize: "12px",
      color: "var(--text-soft)",
      userSelect: "none"
    }}>
      <div 
        onClick={() => {
          setIsExpanded(!isExpanded);
          setHasManuallyToggled(true);
        }}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "6px",
          cursor: "pointer",
          transition: "color 0.15s ease",
          fontWeight: "500",
        }}
        onMouseEnter={(e) => { e.currentTarget.style.color = "var(--text-muted)"; }}
        onMouseLeave={(e) => { e.currentTarget.style.color = "var(--text-soft)"; }}
      >
        <span>{label}</span>
        {isExpanded ? <ChevronDown size={12} style={{ opacity: 0.8 }} /> : <ChevronRight size={12} style={{ opacity: 0.8 }} />}
      </div>

      {!isExpanded && visibleItems.length > 0 && (
        <div style={{
          marginTop: "5px",
          paddingLeft: "16px",
          color: "var(--text-muted)",
          fontSize: "12px",
          lineHeight: 1.5,
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
              <div style={{ display: "flex", alignItems: "center", gap: "6px", color: "var(--accent)", fontSize: "11.75px", fontStyle: "italic" }}>
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
        onClick={() => node.body && setIsExpanded(!isExpanded)}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "6px",
          cursor: node.body ? "pointer" : "default",
          transition: "color 0.15s ease",
          fontWeight: "500",
        }}
        onMouseEnter={(e) => { if (node.body) e.currentTarget.style.color = "var(--text-muted)"; }}
        onMouseLeave={(e) => { if (node.body) e.currentTarget.style.color = "var(--text-soft)"; }}
      >
        <span>{label}</span>
        {(node.filename || parsed.filename) ? getFileIcon(node.filename || parsed.filename) : null}
        {(node.filename || parsed.filename) && (
          <span style={{ color: "var(--text-muted)", fontWeight: "520" }}>{node.filename || parsed.filename}</span>
        )}
        {addCount && <span style={{ color: "#34d399", fontWeight: "600", marginLeft: "4px" }}>{addCount}</span>}
        {delCount && <span style={{ color: "#f87171", fontWeight: "600", marginLeft: "2px" }}>{delCount}</span>}
        {node.body && (
          isExpanded ? <ChevronDown size={12} style={{ opacity: 0.8 }} /> : <ChevronRight size={12} style={{ opacity: 0.8 }} />
        )}
      </div>

      {isExpanded && (node.body || parsed.diffPreview) && (
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
            {parsed.diffPreview || node.body}
          </pre>
        </div>
      )}
    </div>
  );
}

function diffCountsForEdit(item: Extract<TimelineItem, { kind: "activity_item" }>) {
  const parsed = parseToolDetails(item as any);
  const raw = item.diff || parsed.diff || "";
  const add = Number(raw.match(/\+(\d+)/)?.[1] || 0);
  const del = Number(raw.match(/-(\d+)/)?.[1] || 0);
  return { add, del, parsed };
}

export function EditSummaryNode({ node, appLang }: { node: Extract<TimelineItem, { kind: "edit_summary" }>; appLang: string }) {
  const isZh = appLang === "zh";
  const [isExpanded, setIsExpanded] = useState(true);
  const rows = node.items.map((item) => ({
    item,
    ...diffCountsForEdit(item)
  }));
  const totalAdd = rows.reduce((sum, row) => sum + row.add, 0);
  const totalDel = rows.reduce((sum, row) => sum + row.del, 0);
  const editedLabel = isZh ? `已编辑 ${rows.length} 个文件` : `Edited ${rows.length} files`;

  return (
    <div className="edit-summary-card">
      <div className="edit-summary-head">
        <div className="edit-summary-badge">
          <FileCode2 size={18} />
        </div>
        <div className="edit-summary-title">
          <strong>{editedLabel}</strong>
          <span>
            <em className="diff-add">+{totalAdd}</em>
            <em className="diff-del">-{totalDel}</em>
          </span>
        </div>
        <div className="edit-summary-actions">
          <button type="button" title={isZh ? "撤销暂未接入" : "Undo is not connected yet"}>
            {isZh ? "撤销" : "Undo"}
          </button>
          <button type="button" title={isZh ? "查看改动" : "Review changes"} onClick={() => setIsExpanded((value) => !value)}>
            {isZh ? "审核" : "Review"}
          </button>
        </div>
      </div>

      <div className="edit-summary-files">
        {rows.map(({ item, parsed, add, del }) => {
          const filename = item.filename || parsed.filename || displayToolName(item.tool);
          return (
            <div className="edit-summary-file" key={item.id}>
              <span className="edit-summary-file-name">
                {filename ? getFileIcon(filename) : null}
                <span>{filename}</span>
              </span>
              <span className="edit-summary-file-stats">
                <em className="diff-add">+{add}</em>
                <em className="diff-del">-{del}</em>
              </span>
            </div>
          );
        })}
      </div>

      {isExpanded ? (
        <div className="edit-summary-diffs">
          {rows.map(({ item, parsed }) => {
            const filename = item.filename || parsed.filename || displayToolName(item.tool);
            const diffText = parsed.diffPreview || item.result || item.body;
            if (!diffText) return null;
            return (
              <details className="edit-diff" key={`${item.id}-diff`}>
                <summary>
                  {filename ? getFileIcon(filename) : null}
                  <span>{filename}</span>
                </summary>
                <pre>{diffText}</pre>
              </details>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
