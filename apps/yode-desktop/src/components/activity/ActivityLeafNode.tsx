import React, { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { getFileIcon, getCommandIcon } from "../FileIcon";
import { parseToolDetails, displayToolName } from "./ToolUtils";

export function ActivityLeafNode({ item, appLang }: { item: any; appLang: string }) {
  const isZh = appLang === "zh";
  const [isExpanded, setIsExpanded] = useState(false);

  const hasBodyOrResult = !!(item.body || item.result);

  if (item.kind === "reasoning") {
    let displayTitle = item.title || "";
    if (displayTitle.includes("已思考")) {
      const match = displayTitle.match(/\d+/);
      displayTitle = match
        ? (isZh ? `思考了 ${match[0]} 秒` : `Thought for ${match[0]}s`)
        : (isZh ? "已思考" : "Thought");
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
            fontSize: "11.75px"
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
            color: "var(--text-muted)",
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
            fontSize: "11.75px"
          }}
        >
          <span>{label}</span>
          {parsed.filename ? getFileIcon(parsed.filename) : parsed.command ? getCommandIcon() : null}
          {(parsed.filename || parsed.command) ? (
            <span style={{ color: "var(--text-muted)", fontWeight: "520" }}>
              {parsed.filename ? `${parsed.filename}${parsed.lineRange}` : parsed.command}
            </span>
          ) : (
            <span style={{ color: "var(--text-muted)", fontWeight: "520" }}>
              {displayToolName(item.tool)}
            </span>
          )}
          {item.count > 1 && (
            <span style={{ color: "var(--text-soft)", fontSize: "10.75px" }}>x{item.count}</span>
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
            borderLeft: "1px solid var(--line-soft)",
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
