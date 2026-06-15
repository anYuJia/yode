import React, { useEffect, useRef, useState } from "react";
import { Bot, Check, Clock3, Copy } from "lucide-react";
import { TimelineItem } from "../../lib/mock";
import { ProcessNoteNode } from "./ProcessNoteNode";
import { ActivityGroupNode, ActivityItemNode, EditSummaryNode } from "../activity/ActivityGroupNode";
import { InlineToolGroup } from "./InlineToolGroup";
import { MarkdownContent } from "./MarkdownContent";
import { ReasoningNode } from "./ReasoningNode";

export function TimelineNode({ item, appLang, isTurnActive }: { item: TimelineItem; appLang: string; isTurnActive?: boolean }) {
  if (item.kind === "boundary" && item.id.startsWith("cancel-")) {
    return (
      <div
        style={{
          maxWidth: "1064px",
          width: "100%",
          margin: "12px auto",
          paddingLeft: "33px",
        }}
      >
        <div
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "8px",
            padding: "6px 12px",
            background: "color-mix(in oklch, var(--warning), transparent 93%)",
            border: "1px solid color-mix(in oklch, var(--warning), transparent 80%)",
            borderRadius: "6px",
            color: "var(--warning)",
            fontSize: "12px",
            fontWeight: "500",
          }}
        >
          <span style={{ display: "inline-block", width: "6px", height: "6px", borderRadius: "50%", background: "var(--warning)" }}></span>
          <span>{item.title}: {item.body}</span>
        </div>
      </div>
    );
  }

  if (item.kind === "boundary" || item.kind === "permission") {
    return null;
  }

  if (item.kind === "process_note") {
    return <ProcessNoteNode note={item} appLang={appLang} />;
  }

  if (item.kind === "activity_group") {
    return (
      <ActivityGroupNode
        group={item}
        appLang={appLang}
        isTurnActive={isTurnActive}
      />
    );
  }

  if (item.kind === "activity_item") {
    return (
      <ActivityItemNode
        node={item}
        appLang={appLang}
      />
    );
  }

  if (item.kind === "edit_summary") {
    return <EditSummaryNode node={item} appLang={appLang} />;
  }

  if (item.kind === "reasoning") {
    if (item.id === "retrying-attempt") {
      return (
        <div
          style={{
            maxWidth: "1064px",
            width: "100%",
            margin: "8px auto 12px",
            padding: "8px 0 8px 33px",
            fontSize: "12px",
            display: "flex",
            flexDirection: "column",
            gap: "4px",
            userSelect: "none"
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "6px", fontWeight: "500", color: "var(--warning)" }}>
            <Clock3 size={13} style={{ animation: "pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite" }} />
            <span>{item.title}</span>
          </div>
          {item.body && (
            <div style={{ color: "var(--text-soft)", fontSize: "11.5px", whiteSpace: "pre-wrap", marginTop: "2px" }}>
              {item.body}
            </div>
          )}
        </div>
      );
    }
    return <ReasoningNode item={item} appLang={appLang} />;
  }

  if (item.kind === "tool") {
    return null;
  }

  if (item.kind === "tool_group") {
    return (
      <InlineToolGroup
        label={item.label}
        items={item.items || []}
        appLang={appLang}
      />
    );
  }

  if (item.kind === "user") {
    return <UserMessageNode item={item} appLang={appLang} />;
  }

  if (item.kind === "assistant" && (item.meta === "intermediate" || item.meta === "streaming")) {
    return (
      <div
        style={{
          maxWidth: "1064px",
          width: "100%",
          margin: "4px auto 12px",
          paddingLeft: "33px",
        }}
      >
        <MarkdownContent text={item.body} variant="process" />
      </div>
    );
  }

  if (item.kind !== "assistant") {
    return null;
  }

  const icon = <Bot size={18} />;

  return (
    <article className={`timeline-node ${item.kind}`}>
      <div className="node-rail">
        <div className="node-icon">{icon}</div>
      </div>
      <div className="node-content">
        <div className="node-header">
          <h2>{item.title}</h2>
        </div>
        {item.kind === "assistant" ? (
          <MarkdownContent text={item.body} variant="answer" />
        ) : "body" in item ? (
          <p>{(item as any).body}</p>
        ) : null}
      </div>
    </article>
  );
}

function formatMessageTime(createdAt?: number, fallback?: string) {
  if (typeof createdAt !== "number" || !Number.isFinite(createdAt)) {
    return fallback || "";
  }
  return new Date(createdAt).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false
  });
}

function UserMessageNode({ item, appLang }: { item: Extract<TimelineItem, { kind: "user" | "assistant" | "reasoning" }>; appLang: string }) {
  const [showActions, setShowActions] = useState(false);
  const [copied, setCopied] = useState(false);
  const hideTimerRef = useRef<number | null>(null);
  const copiedTimerRef = useRef<number | null>(null);

  const clearHideTimer = () => {
    if (hideTimerRef.current !== null) {
      window.clearTimeout(hideTimerRef.current);
      hideTimerRef.current = null;
    }
  };

  const show = () => {
    clearHideTimer();
    setShowActions(true);
  };

  const scheduleHide = () => {
    clearHideTimer();
    hideTimerRef.current = window.setTimeout(() => {
      setShowActions(false);
    }, 180);
  };

  useEffect(() => {
    return () => {
      clearHideTimer();
      if (copiedTimerRef.current !== null) {
        window.clearTimeout(copiedTimerRef.current);
      }
    };
  }, []);

  const copyText = async () => {
    try {
      await navigator.clipboard.writeText(item.body || "");
      setCopied(true);
      if (copiedTimerRef.current !== null) window.clearTimeout(copiedTimerRef.current);
      copiedTimerRef.current = window.setTimeout(() => setCopied(false), 1200);
    } catch (err) {
      console.error(err);
    }
  };

  const timeText = formatMessageTime(item.createdAt, item.meta);
  const copyLabel = appLang === "zh" ? "复制消息" : "Copy message";

  return (
    <div
      className="timeline-node user-bubble-container"
      onMouseEnter={show}
      onMouseLeave={scheduleHide}
      onFocus={show}
      onBlur={scheduleHide}
    >
      <div className="user-message-stack">
        <div className="user-chat-bubble">
          <p>{item.body}</p>
          {item.attachments && item.attachments.length > 0 ? (
            <div className="message-image-grid">
              {item.attachments.map((image) => (
                <img
                  key={image.id}
                  src={image.dataUrl}
                  alt={image.name}
                  title={image.name}
                />
              ))}
            </div>
          ) : null}
        </div>
        <div
          className={`user-message-actions ${showActions ? "visible" : ""}`}
          onMouseEnter={show}
          onMouseLeave={scheduleHide}
        >
          {timeText ? <span className="user-message-time">{timeText}</span> : null}
          <button type="button" className="user-message-copy" onClick={copyText} aria-label={copyLabel} title={copyLabel}>
            {copied ? <Check size={15} /> : <Copy size={15} />}
          </button>
        </div>
      </div>
    </div>
  );
}
