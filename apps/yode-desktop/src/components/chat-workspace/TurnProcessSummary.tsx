import React, { useState, useEffect, useRef } from "react";
import { CircleDot, Check, ChevronDown } from "lucide-react";
import { formatDurationZh } from "../../lib/timelineUtils";

export function TurnProcessSummary({ turnId, isActive, isWaitingForUser, isExpanded, onToggle, durationSeconds, processCount, appLang }: {
  turnId: string;
  isActive: boolean;
  isWaitingForUser?: boolean;
  isExpanded: boolean;
  onToggle: () => void;
  durationSeconds: number;
  processCount: number;
  appLang: string;
}) {
  const [elapsed, setElapsed] = useState(durationSeconds);
  const startRef = useRef<number | null>(null);
  const isZh = appLang === "zh";

  useEffect(() => {
    if (!isActive) {
      setElapsed(durationSeconds);
      return;
    }

    if (startRef.current === null) {
      startRef.current = Date.now() - durationSeconds * 1000;
    }
    const start = startRef.current;

    setElapsed(Math.floor((Date.now() - start) / 1000));

    const timer = setInterval(() => {
      setElapsed(Math.floor((Date.now() - start) / 1000));
    }, 1000);

    return () => clearInterval(timer);
  }, [turnId, isActive, durationSeconds]);

  const durationText = isZh ? formatDurationZh(elapsed) : `${elapsed}s`;
  const title = isActive
    ? isWaitingForUser
      ? isZh
        ? `等待回复 ${durationText}`
        : `Waiting for reply ${durationText}`
      : isZh
        ? `处理中 ${durationText}`
        : `Working for ${durationText}`
    : isZh
      ? `已处理 ${durationText}`
      : `Task finished in ${durationText}`;
  const detail = isActive
    ? isWaitingForUser
      ? isZh
        ? "请选择或输入答案"
        : "Choose or type an answer"
      : isZh
        ? "过程正在展开"
        : "Process is visible"
    : isExpanded
      ? (isZh ? "收起过程" : "Collapse process")
      : (isZh ? `展开过程（${processCount} 项）` : `Show process (${processCount})`);

  return (
    <button
      onClick={onToggle}
      className={`turn-process-summary ${isActive ? "running" : "complete"} ${isExpanded ? "expanded" : "collapsed"}`}
      type="button"
      aria-expanded={isExpanded}
      aria-label={detail}
    >
      <span className="turn-process-summary-icon">
        {isActive ? <CircleDot size={9} className="glowing-logo" /> : <Check size={11} />}
      </span>
      <span className="turn-process-summary-main">{title}</span>
      <span className="turn-process-summary-detail">{detail}</span>
      <ChevronDown size={13} className="turn-process-summary-chevron" />
    </button>
  );
}
