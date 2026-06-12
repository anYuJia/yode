import React from "react";
import { TimelineItem } from "../lib/mock";

interface RunInspectorProps {
  isProcessing: boolean;
  permissionMode: string;
  timelineItems: TimelineItem[];
}

export function RunInspector({
  isProcessing,
  permissionMode,
  timelineItems
}: RunInspectorProps) {
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
