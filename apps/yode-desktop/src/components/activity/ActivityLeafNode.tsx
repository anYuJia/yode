import React, { useLayoutEffect, useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { getFileIcon, getCommandIcon } from "../FileIcon";
import { getActivityDescriptor, displayToolName } from "./ToolUtils";

function notifyTimelineLayoutChanged() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent("yode:timeline-layout-change"));
}

function parseJsonObject(text?: string): any | null {
  if (!text) return null;
  try {
    const parsed = JSON.parse(text);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function splitReadableText(text?: string) {
  return (text || "")
    .split(/\n{2,}/)
    .map((paragraph) => paragraph.trim())
    .filter(Boolean);
}

function PlanModeDetail({ item, appLang }: { item: any; appLang: string }) {
  const isZh = appLang === "zh";
  const params = parseJsonObject(item.body);
  const allowedPrompts = Array.isArray(params?.allowedPrompts)
    ? params.allowedPrompts.filter((entry: any) => entry && typeof entry === "object")
    : [];
  const isExit = item.tool === "exit_plan_mode";
  const planSteps = isZh
    ? [
      "只探索代码库和现有模式",
      "比较实现方案和取舍",
      "需要确认时向用户提问",
      "整理清晰计划后再开始编码"
    ]
    : [
      "Explore the codebase and existing patterns",
      "Compare implementation options and trade-offs",
      "Ask the user when the approach needs clarification",
      "Prepare a clear plan before editing files"
    ];
  const exitNotes = isZh
    ? [
      "计划已经确认，可以进入实施阶段。",
      "如有待办列表，先更新任务状态，再开始修改代码。"
    ]
    : [
      "The plan has been approved and implementation can begin.",
      "Update the todo list first if one is in use, then start editing."
    ];

  return (
    <div className={`activity-tool-card ${isExit ? "success" : "info"}`}>
      <div className="activity-tool-card-head">
        <span className="activity-tool-card-kicker">
          {isExit ? (isZh ? "计划已确认" : "Plan approved") : (isZh ? "只读规划" : "Read-only planning")}
        </span>
        <span className="activity-tool-card-title">
          {isExit
            ? (isZh ? "可以开始实施" : "Ready to implement")
            : (isZh ? "进入探索和设计阶段" : "Exploration and design mode")}
        </span>
      </div>

      {allowedPrompts.length > 0 ? (
        <div className="activity-tool-section">
          <div className="activity-tool-section-label">
            {isZh ? "已允许的操作范围" : "Allowed action scope"}
          </div>
          <div className="activity-tool-chips">
            {allowedPrompts.map((entry: any, index: number) => (
              <span key={`${entry.tool || "tool"}-${index}`} className="activity-tool-chip">
                <span>{entry.tool || "tool"}</span>
                {entry.prompt ? <strong>{String(entry.prompt)}</strong> : null}
              </span>
            ))}
          </div>
        </div>
      ) : null}

      {isExit ? (
        <div className="activity-tool-section">
          {exitNotes.map((paragraph, index) => (
            <p key={index} className="activity-tool-paragraph">{paragraph}</p>
          ))}
        </div>
      ) : (
        <ol className="activity-tool-list">
          {planSteps.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
      )}
    </div>
  );
}

function JsonDetail({ text, appLang, tone = "neutral" }: { text: string; appLang: string; tone?: "neutral" | "input" }) {
  const isZh = appLang === "zh";
  const parsed = parseJsonObject(text);
  if (!parsed) {
    return (
      <div className="activity-tool-text">
        {splitReadableText(text).map((paragraph, index) => (
          <p key={index}>{paragraph}</p>
        ))}
      </div>
    );
  }

  const entries = Object.entries(parsed).filter(([, value]) => value !== undefined && value !== null && value !== "");
  if (entries.length === 0) {
    return (
      <div className={`activity-tool-empty ${tone}`}>
        {isZh ? "无需额外参数" : "No extra parameters"}
      </div>
    );
  }

  return (
    <div className="activity-tool-fields">
      {entries.map(([key, value]) => (
        <div key={key} className="activity-tool-field">
          <span>{key}</span>
          <strong>
            {typeof value === "string"
              ? value
              : Array.isArray(value)
                ? `${value.length} ${isZh ? "项" : "items"}`
                : JSON.stringify(value)}
          </strong>
        </div>
      ))}
    </div>
  );
}

function ToolDetail({ item, appLang }: { item: any; appLang: string }) {
  if (item.tool === "enter_plan_mode" || item.tool === "exit_plan_mode") {
    return <PlanModeDetail item={item} appLang={appLang} />;
  }

  const bodyIsJson = Boolean(parseJsonObject(item.body));
  const resultIsJson = Boolean(parseJsonObject(item.result));

  if (bodyIsJson || resultIsJson) {
    return (
      <div className="activity-tool-card neutral">
        {item.body ? (
          <div className="activity-tool-section">
            <div className="activity-tool-section-label">{appLang === "zh" ? "参数" : "Parameters"}</div>
            <JsonDetail text={item.body} appLang={appLang} tone="input" />
          </div>
        ) : null}
        {item.result ? (
          <div className="activity-tool-section">
            <div className="activity-tool-section-label">{appLang === "zh" ? "结果" : "Result"}</div>
            <JsonDetail text={item.result} appLang={appLang} />
          </div>
        ) : null}
      </div>
    );
  }

  return (
    <>
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
    </>
  );
}

export function ActivityLeafNode({ item, appLang }: { item: any; appLang: string }) {
  const isZh = appLang === "zh";
  const [isExpanded, setIsExpanded] = useState(false);

  const hasBodyOrResult = !!(item.body || item.result);

  useLayoutEffect(() => {
    notifyTimelineLayoutChanged();
    const firstFrame = window.requestAnimationFrame(() => {
      notifyTimelineLayoutChanged();
      window.requestAnimationFrame(notifyTimelineLayoutChanged);
    });
    return () => window.cancelAnimationFrame(firstFrame);
  }, [isExpanded]);

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
            <ToolDetail item={item} appLang={appLang} />
          </div>
        )}
      </div>
    );
  }

  return null;
}
