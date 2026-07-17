import React from "react";
import { TimelineItem, UsageSnapshot } from "../lib/desktopTypes";

interface RunInspectorProps {
  isProcessing: boolean;
  permissionMode: string;
  timelineItems: TimelineItem[];
  usageSnapshot: UsageSnapshot | null;
  appLang: string;
}

function toolStatusLabel(status: "running" | "success" | "blocked", isZh: boolean): string {
  if (status === "running") return isZh ? "运行中" : "running";
  if (status === "success") return isZh ? "完成" : "done";
  return isZh ? "阻塞" : "blocked";
}

export function RunInspector({
  isProcessing,
  permissionMode,
  timelineItems,
  usageSnapshot,
  appLang
}: RunInspectorProps) {
  const isZh = appLang === "zh";
  const toolItems = timelineItems.filter((item) => item.kind === "tool");
  const completedToolItems = toolItems.filter((item) => item.status !== "running");
  const runningToolItems = toolItems.filter((item) => item.status === "running");
  const recentToolItems = [...toolItems].reverse().slice(0, 10);

  const totalTokens = usageSnapshot?.totalTokens ?? 0;
  const estimatedCost = usageSnapshot?.estimatedCost ?? 0;

  return (
    <aside className="run-inspector" aria-label={isZh ? "运行详情" : "Run details"}>
      <div className="inspector-head">
        <span>TURN</span>
        <strong>
          {timelineItems.length} {isZh ? "事件" : "events"}
        </strong>
      </div>

      <div className="inspector-section">
        <div className="metric-row">
          <span>{isZh ? "状态" : "Status"}</span>
          <strong className={isProcessing ? "state-live" : ""}>
            {isProcessing ? (isZh ? "流式中" : "streaming") : isZh ? "空闲" : "idle"}
          </strong>
        </div>
        <div className="metric-row">
          <span>{isZh ? "权限" : "Permission"}</span>
          <strong>{permissionMode}</strong>
        </div>
        <div className="metric-row">
          <span>{isZh ? "上下文" : "Context"}</span>
          <strong>
            {timelineItems.length > 0 ? (isZh ? "活跃" : "active") : isZh ? "空" : "empty"}
          </strong>
        </div>
        <div className="metric-row">
          <span>{isZh ? "工具" : "Tools"}</span>
          <strong>
            {completedToolItems.length} / {toolItems.length}
          </strong>
        </div>
        {usageSnapshot ? (
          <div className="metric-row">
            <span>Token</span>
            <strong>{totalTokens}</strong>
          </div>
        ) : null}
        {usageSnapshot && estimatedCost > 0 ? (
          <div className="metric-row">
            <span>{isZh ? "预估成本" : "Est. cost"}</span>
            <strong>${estimatedCost.toFixed(4)}</strong>
          </div>
        ) : null}
      </div>

      {runningToolItems.length > 0 ? (
        <div className="inspector-section">
          <span className="inspector-label">{isZh ? "运行中工具" : "Running tools"}</span>
          {runningToolItems.map((item) => (
            <div key={item.id} className="file-row">
              <span className="tool-name">{item.tool}</span>
              <span className="tool-status running">{toolStatusLabel(item.status, isZh)}</span>
            </div>
          ))}
        </div>
      ) : null}

      {recentToolItems.length > 0 ? (
        <div className="inspector-section">
          <span className="inspector-label">
            {isZh ? "最近工具" : "Recent tools"}
            <span className="inspector-count"> ({recentToolItems.length})</span>
          </span>
          {recentToolItems.map((item) => (
            <div key={item.id} className="file-row">
              <span className="tool-name" title={item.title || item.tool}>
                {item.tool}
              </span>
              <span className={`tool-status ${item.status}`}>
                {toolStatusLabel(item.status, isZh)}
              </span>
            </div>
          ))}
        </div>
      ) : null}

      {timelineItems.length === 0 ? (
        <div className="inspector-section">
          <span className="inspector-label">{isZh ? "等待会话" : "Waiting for session"}</span>
          <p>
            {isZh
              ? "选择会话或发送消息继续。"
              : "Select a session or send a message to continue."}
          </p>
        </div>
      ) : null}
    </aside>
  );
}
