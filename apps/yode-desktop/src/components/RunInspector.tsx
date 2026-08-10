import React, { useCallback, useEffect, useMemo, useState } from "react";
import { Copy, RefreshCw, TriangleAlert } from "lucide-react";

import { turnRecentEvents } from "../lib/desktopIpc";
import type { RunState, TimelineItem, TurnEventRecord, UsageSnapshot } from "../lib/desktopTypes";
import { TERMINAL_RUN_STATUSES } from "../lib/desktopTypes";

interface RunInspectorProps {
  isProcessing: boolean;
  permissionMode: string;
  timelineItems: TimelineItem[];
  usageSnapshot: UsageSnapshot | null;
  appLang: string;
  /** 当前会话的持久化 run journal（runs_list 投影）。 */
  currentRun: RunState | null;
  replayState: { status: "idle" | "loading" | "done" | "error"; error?: string };
  onRetryReplay?: () => void;
}

const MAX_EVENT_BODY_CHARS = 400;

/** 事件详情安全截断：不泄漏密钥（payload 已在落盘时脱敏，展示层仍限制长度）。 */
export function truncateBody(body: string): string {
  if (body.length <= MAX_EVENT_BODY_CHARS) return body;
  return `${body.slice(0, MAX_EVENT_BODY_CHARS)}…`;
}

export function formatDuration(ms: number, isZh: boolean): string {
  const safeMs = Math.max(0, ms);
  const seconds = Math.floor(safeMs / 1000);
  if (seconds < 1) return isZh ? "刚刚开始" : "just started";
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  if (minutes <= 0) return isZh ? `${rest} 秒` : `${rest}s`;
  if (rest <= 0) return isZh ? `${minutes} 分钟` : `${minutes}m`;
  return isZh ? `${minutes} 分 ${rest} 秒` : `${minutes}m ${rest}s`;
}

export function formatTimestamp(timestamp: string, isZh: boolean): string {
  const time = new Date(timestamp).getTime();
  if (!Number.isFinite(time)) return timestamp;
  return new Intl.DateTimeFormat(isZh ? "zh-CN" : "en-US", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  }).format(time);
}

export function runStatusLabel(status: string, isZh: boolean): string {
  const labels: Record<string, { zh: string; en: string }> = {
    idle: { zh: "空闲", en: "idle" },
    starting: { zh: "启动中", en: "starting" },
    running: { zh: "运行中", en: "running" },
    waiting_approval: { zh: "等待授权", en: "waiting approval" },
    waiting_user: { zh: "等待用户", en: "waiting for user" },
    cancelling: { zh: "取消中", en: "cancelling" },
    completed: { zh: "已完成", en: "completed" },
    cancelled: { zh: "已取消", en: "cancelled" },
    failed: { zh: "失败", en: "failed" },
    interrupted: { zh: "已中断", en: "interrupted" }
  };
  const label = labels[status];
  return label ? (isZh ? label.zh : label.en) : status;
}

function toolStatusLabel(status: "running" | "success" | "blocked", isZh: boolean): string {
  if (status === "running") return isZh ? "运行中" : "running";
  if (status === "success") return isZh ? "完成" : "done";
  return isZh ? "阻塞" : "blocked";
}

/** 单条持久化事件的折叠展示：默认折叠，可展开、复制、安全截断。 */
function EventDetailRow({ event, isZh }: { event: TurnEventRecord; isZh: boolean }) {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const body = useMemo(() => {
    const raw = typeof event.payload === "string" ? event.payload : JSON.stringify(event.payload, null, 2);
    return raw ?? "";
  }, [event.payload]);
  const preview = useMemo(() => truncateBody(body), [body]);

  const copy = useCallback(() => {
    void navigator.clipboard?.writeText(body).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    });
  }, [body]);

  const canExpand = body.length > MAX_EVENT_BODY_CHARS;

  return (
    <div className="inspector-event-row">
      <button
        type="button"
        className="inspector-event-summary"
        onClick={() => setExpanded((current) => !current)}
        aria-expanded={expanded}
      >
        <span className="inspector-event-kind">{event.kind}</span>
        <span className="inspector-event-seq">#{event.seq}</span>
        <span className="inspector-event-time">{formatTimestamp(event.timestamp, isZh)}</span>
      </button>
      <div className={`inspector-event-body ${expanded ? "expanded" : ""}`}>
        <pre>{expanded ? body : preview}</pre>
        <div className="inspector-event-actions">
          <button type="button" onClick={copy}>
            {copied ? (isZh ? "已复制" : "copied") : isZh ? "复制" : "copy"}
          </button>
          {canExpand ? (
            <button type="button" onClick={() => setExpanded((current) => !current)}>
              {expanded ? (isZh ? "收起" : "collapse") : isZh ? "展开" : "expand"}
            </button>
          ) : null}
        </div>
      </div>
    </div>
  );
}


/** RunInspector 的可测试视图模型：状态/等待原因/诊断/恢复状态渲染所需的全部派生数据。 */
export type RunInspectorViewModel = {
  status: string;
  statusLabel: string;
  waitingReason: string | null;
  replayLoading: boolean;
  replayError: string | null;
  showDiagnostics: boolean;
  errorCode: string | null;
  detail: string | null;
  hasJournal: boolean;
};

export function runInspectorViewModel(
  currentRun: RunState | null,
  isProcessing: boolean,
  replayState: { status: "idle" | "loading" | "done" | "error"; error?: string },
  isZh: boolean
): RunInspectorViewModel {
  const status = currentRun?.status ?? (isProcessing ? "running" : "idle");
  const waitingReason =
    currentRun?.status === "waiting_approval"
      ? isZh
        ? "等待工具执行授权"
        : "awaiting tool approval"
      : currentRun?.status === "waiting_user"
        ? isZh
          ? "等待用户回答问题"
          : "awaiting your answer"
        : currentRun?.status === "cancelling"
          ? isZh
            ? "正在停止本轮运行"
            : "stopping the current run"
          : null;
  return {
    status,
    statusLabel: runStatusLabel(status, isZh),
    waitingReason,
    replayLoading: replayState.status === "loading",
    replayError: replayState.status === "error" ? (replayState.error ?? (isZh ? "事件重放失败，当前状态已锁定。" : "Event replay failed; state stays locked.")) : null,
    showDiagnostics: currentRun !== null && TERMINAL_RUN_STATUSES.has(currentRun.status) && Boolean(currentRun.detail || currentRun.errorCode),
    errorCode: currentRun?.errorCode ?? null,
    detail: currentRun?.detail ?? null,
    hasJournal: currentRun !== null
  };
}

export function RunInspector({
  isProcessing,
  permissionMode,
  timelineItems,
  usageSnapshot,
  appLang,
  currentRun,
  replayState,
  onRetryReplay
}: RunInspectorProps) {
  const isZh = appLang === "zh";
  const [recentEvents, setRecentEvents] = useState<TurnEventRecord[]>([]);
  const [eventsLoading, setEventsLoading] = useState(false);
  const [eventsError, setEventsError] = useState(false);
  const [eventsReloadKey, setEventsReloadKey] = useState(0);

  const toolItems = useMemo(() => timelineItems.filter((item) => item.kind === "tool"), [timelineItems]);
  const completedToolItems = useMemo(() => toolItems.filter((item) => item.status !== "running"), [toolItems]);
  const runningToolItems = useMemo(() => toolItems.filter((item) => item.status === "running"), [toolItems]);
  const recentToolItems = useMemo(() => [...toolItems].reverse().slice(0, 10), [toolItems]);

  const totalTokens = usageSnapshot?.totalTokens ?? 0;
  const estimatedCost = usageSnapshot?.estimatedCost ?? 0;

  const runKey = currentRun ? `${currentRun.sessionId}:${currentRun.turnId}` : null;

  // 消费持久化 turn journal：当前 turn 的最近事件（payload 已脱敏，可安全展示）
  useEffect(() => {
    if (!currentRun || !runKey) {
      setRecentEvents([]);
      return;
    }
    let active = true;
    setEventsLoading(true);
    setEventsError(false);
    turnRecentEvents(currentRun.sessionId, currentRun.turnId, 20)
      .then((events) => {
        if (!active) return;
        setRecentEvents(events);
        setEventsError(false);
      })
      .catch((err) => {
        console.error(err);
        if (!active) return;
        setEventsError(true);
      })
      .finally(() => {
        if (active) setEventsLoading(false);
      });
    return () => {
      active = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runKey, eventsReloadKey, currentRun?.lastSeq]);

  const viewModel = useMemo(
    () => runInspectorViewModel(currentRun, isProcessing, replayState, isZh),
    [currentRun, isProcessing, replayState, isZh]
  );
  const { status, waitingReason } = viewModel;
  const isTerminalRun = currentRun !== null && TERMINAL_RUN_STATUSES.has(currentRun.status);
  const startedAt = currentRun?.startedAt ? new Date(currentRun.startedAt).getTime() : null;
  const endedAt = currentRun?.endedAt ? new Date(currentRun.endedAt).getTime() : null;
  const durationMs =
    endedAt && startedAt ? endedAt - startedAt : startedAt && status !== "idle" ? Date.now() - startedAt : 0;

  return (
    <aside className="run-inspector" aria-label={isZh ? "运行详情" : "Run details"}>
      <div className="inspector-head">
        <span>TURN</span>
        <strong>
          {timelineItems.length} {isZh ? "事件" : "events"}
        </strong>
      </div>

      {/* 断线恢复失败：保留锁定状态，提供可重试路径 */}
      {viewModel.replayError ? (
        <div className="inspector-section inspector-warning">
          <span className="inspector-label">
            <TriangleAlert size={12} /> {isZh ? "恢复失败" : "Recovery failed"}
          </span>
          <p>{viewModel.replayError}</p>
          {onRetryReplay ? (
            <button type="button" className="inspector-action-button" onClick={onRetryReplay}>
              <RefreshCw size={11} /> {isZh ? "重试恢复" : "Retry recovery"}
            </button>
          ) : null}
        </div>
      ) : null}

      {viewModel.replayLoading ? (
        <div className="inspector-section">
          <span className="inspector-label">{isZh ? "正在恢复" : "Recovering"}</span>
          <p>{isZh ? "正在重放上次运行事件…" : "Replaying events from the last run…"}</p>
        </div>
      ) : null}

      <div className="inspector-section">
        <div className="metric-row">
          <span>{isZh ? "状态" : "Status"}</span>
          <strong className={status !== "idle" && !isTerminalRun ? "state-live" : ""}>
            {runStatusLabel(status, isZh)}
          </strong>
        </div>
        {waitingReason ? (
          <div className="metric-row">
            <span>{isZh ? "等待原因" : "Waiting"}</span>
            <strong className="state-live">{waitingReason}</strong>
          </div>
        ) : null}
        {startedAt ? (
          <div className="metric-row">
            <span>{isZh ? "开始时间" : "Started"}</span>
            <strong>{formatTimestamp(currentRun?.startedAt ?? "", isZh)}</strong>
          </div>
        ) : null}
        {durationMs > 0 ? (
          <div className="metric-row">
            <span>{isZh ? "持续时间" : "Duration"}</span>
            <strong>{formatDuration(durationMs, isZh)}</strong>
          </div>
        ) : null}
        <div className="metric-row">
          <span>{isZh ? "权限" : "Permission"}</span>
          <strong>{permissionMode}</strong>
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

      {/* interrupted/cancelled/failed 的诊断信息（来自持久化 journal） */}
      {viewModel.showDiagnostics && currentRun ? (
        <div className="inspector-section">
          <span className="inspector-label">
            {currentRun.status === "interrupted"
              ? isZh
                ? "中断诊断"
                : "Interrupted diagnostics"
              : currentRun.status === "cancelled"
                ? isZh
                  ? "取消诊断"
                  : "Cancellation diagnostics"
                : isZh
                  ? "错误详情"
                  : "Error details"}
          </span>
          {viewModel.errorCode ? (
            <div className="metric-row">
              <span>{isZh ? "错误码" : "Code"}</span>
              <strong>{viewModel.errorCode}</strong>
            </div>
          ) : null}
          {viewModel.detail ? (
            <p className="inspector-diagnostic">{viewModel.detail}</p>
          ) : null}
        </div>
      ) : null}

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

      {/* 持久化事件流（来自 turn journal，事件详情支持折叠/复制/截断） */}
      {currentRun ? (
        <div className="inspector-section">
          <span className="inspector-label">
            {isZh ? "最近事件" : "Recent events"}
            {eventsLoading ? (
              <span className="inspector-count"> {isZh ? "加载中…" : "loading…"}</span>
            ) : (
              <button
                type="button"
                className="inspector-refresh"
                aria-label={isZh ? "重新加载事件" : "Reload events"}
                onClick={() => setEventsReloadKey((key) => key + 1)}
              >
                <RefreshCw size={11} />
              </button>
            )}
          </span>
          {eventsError ? (
            <div className="inspector-section inspector-warning">
              <p>{isZh ? "事件读取失败，请重试。" : "Failed to load events. Please retry."}</p>
              <button
                type="button"
                className="inspector-action-button"
                onClick={() => setEventsReloadKey((key) => key + 1)}
              >
                <RefreshCw size={11} /> {isZh ? "重试" : "Retry"}
              </button>
            </div>
          ) : null}
          {!eventsLoading && !eventsError && recentEvents.length === 0 ? (
            <p className="inspector-empty">{isZh ? "暂无已持久化事件" : "No persisted events yet"}</p>
          ) : null}
          {recentEvents.map((event) => (
            <EventDetailRow key={`${event.sessionId}:${event.turnId}:${event.seq}`} event={event} isZh={isZh} />
          ))}
        </div>
      ) : null}

      {timelineItems.length === 0 && !currentRun ? (
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
