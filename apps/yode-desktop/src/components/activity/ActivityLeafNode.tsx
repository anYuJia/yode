import React, { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { getFileIcon, getCommandIcon } from "../FileIcon";
import { getActivityDescriptor, displayToolName } from "./ToolUtils";

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
      <div className="activity-leaf">
        <div 
          onClick={() => item.body && setIsExpanded(!isExpanded)}
          className={`activity-leaf-trigger ${item.body ? "interactive" : ""}`}
        >
          <span>{displayTitle}</span>
          {item.body && (
            isExpanded ? <ChevronDown size={11} className="activity-chevron" /> : <ChevronRight size={11} className="activity-chevron" />
          )}
        </div>
        {isExpanded && item.body && (
          <div className="activity-leaf-detail activity-leaf-detail-reasoning">
            {item.body}
          </div>
        )}
      </div>
    );
  }

  if (item.kind === "tool") {
    const descriptor = getActivityDescriptor(item);
    const isRunning = item.status === "running";
    
    let label = "";
    let value = descriptor.filename
      ? `${descriptor.filename}${descriptor.lineRange || ""}`
      : descriptor.command || descriptor.target;
    if (descriptor.kind === "read" || descriptor.kind === "search") {
      label = isRunning ? (isZh ? "正在读取" : "Reading") : (isZh ? "已读取" : "Read");
      if (descriptor.kind === "search") {
        label = isRunning ? (isZh ? "正在搜索" : "Searching") : (isZh ? "已搜索" : "Searched");
      }
    } else if (descriptor.kind === "run") {
      label = isRunning 
        ? (isZh ? "正在运行命令" : "Running command") 
        : (isZh ? "已运行" : "Ran");
    } else if (descriptor.kind === "edit") {
      label = isRunning
        ? (isZh ? "正在修改" : "Editing")
        : (isZh ? "已修改" : "Edited");
    } else {
      label = isRunning
        ? (isZh ? "正在执行" : "Executing")
        : (isZh ? "已执行" : "Executed");
      if (!value) value = displayToolName(item.tool);
    }

    return (
      <div className="activity-leaf">
        <div 
          onClick={() => hasBodyOrResult && setIsExpanded(!isExpanded)}
          className={`activity-leaf-trigger ${hasBodyOrResult ? "interactive" : ""}`}
        >
          <span>{label}</span>
          {descriptor.filename ? getFileIcon(descriptor.filename) : descriptor.command ? getCommandIcon() : null}
          {value ? (
            <span className="activity-strong">
              {value}
            </span>
          ) : (
            <span className="activity-strong">
              {displayToolName(item.tool)}
            </span>
          )}
          {item.count > 1 && (
            <span className="activity-leaf-count">x{item.count}</span>
          )}
          {hasBodyOrResult && (
            isExpanded ? <ChevronDown size={11} className="activity-chevron" /> : <ChevronRight size={11} className="activity-chevron" />
          )}
        </div>
        
        {isExpanded && (
          <div className="activity-leaf-expanded">
            {item.body && (
              <div>
                <pre className="activity-leaf-code activity-leaf-code-input">
                  {item.body}
                </pre>
              </div>
            )}
            {item.result && (
              <div>
                <pre className="activity-leaf-code activity-leaf-code-result">
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
