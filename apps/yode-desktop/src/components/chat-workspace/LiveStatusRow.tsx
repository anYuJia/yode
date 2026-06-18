import React, { useEffect, useMemo, useState } from "react";
import { CircleDot } from "lucide-react";
import { TimelineItem } from "../../lib/desktopTypes";

const ZH_FALLBACKS = [
  "正在检查刚才的改动...",
  "正在汇总工具结果...",
  "正在确认执行结果...",
  "正在整理下一步..."
];

const EN_FALLBACKS = [
  "Checking the latest changes...",
  "Reviewing tool results...",
  "Confirming the run output...",
  "Preparing the next step..."
];

export function liveStatusTextForItems(items: TimelineItem[], appLang: string) {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    const item = items[index];
    if (item.kind === "reasoning" && item.meta === "running") return appLang === "zh" ? "思考中..." : "Thinking...";
    if (item.kind === "process_note" && item.status === "running") return item.title || (appLang === "zh" ? "处理中..." : "Working...");
    if (item.kind === "edit_summary") return appLang === "zh" ? "正在检查刚才的改动..." : "Checking the latest edits...";
    if (item.kind === "activity_group") return appLang === "zh" ? "正在汇总工具结果..." : "Reviewing tool results...";
    if (item.kind === "tool_group") return appLang === "zh" ? "正在确认执行结果..." : "Confirming tool output...";
    if (item.kind === "assistant" && item.meta === "streaming") return appLang === "zh" ? "正在组织回复..." : "Writing the response...";
  }
  return "";
}

export function LiveStatusRow({ items, appLang, waitingForUser }: { items: TimelineItem[]; appLang: string; waitingForUser?: boolean }) {
  const [tick, setTick] = useState(0);
  const fallbacks = appLang === "zh" ? ZH_FALLBACKS : EN_FALLBACKS;

  useEffect(() => {
    const timer = window.setInterval(() => setTick((value) => value + 1), 2600);
    return () => window.clearInterval(timer);
  }, []);

  const text = useMemo(() => {
    if (waitingForUser) return appLang === "zh" ? "正在等待你的选择..." : "Waiting for your choice...";
    return liveStatusTextForItems(items, appLang) || fallbacks[tick % fallbacks.length];
  }, [items, appLang, waitingForUser, fallbacks, tick]);

  return (
    <div className="live-status-row" aria-live="polite">
      <CircleDot size={10} className="process-pulse-dot" />
      <span key={text}>{text}</span>
    </div>
  );
}
