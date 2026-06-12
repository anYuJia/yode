import React from "react";
import { Bot, Clock3, CircleDot } from "lucide-react";
import { TimelineItem } from "../../lib/mock";
import { ProcessNoteNode } from "./ProcessNoteNode";
import { ActivityGroupNode, ActivityItemNode, EditSummaryNode } from "../activity/ActivityGroupNode";
import { InlineToolGroup } from "./InlineToolGroup";
import { MarkdownContent } from "./MarkdownContent";

export function TimelineNode({ item, appLang, isTurnActive }: { item: TimelineItem; appLang: string; isTurnActive?: boolean }) {
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

  if (item.kind === "reasoning" && item.id === "retrying-attempt") {
    return (
      <div 
        style={{ 
          maxWidth: "760px",
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

  if (item.kind === "tool" || item.kind === "reasoning") {
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
    return (
      <div 
        className="timeline-node user-bubble-container" 
        style={{ 
          display: "flex", 
          justifyContent: "flex-end", 
          width: "100%", 
          maxWidth: "760px",
          margin: "0 auto 12px",
          paddingLeft: "24px"
        }}
      >
        <div 
          className="user-chat-bubble"
          style={{
            background: "color-mix(in oklch, var(--accent), transparent 85%)",
            border: "none",
            borderRadius: "14px 14px 2px 14px",
            padding: "10px 14px",
            maxWidth: "85%",
            boxShadow: "0 2px 8px rgba(0, 0, 0, 0.15)",
            display: "block",
            overflow: "hidden"
          }}
        >
          <p style={{ 
            margin: 0, 
            color: "var(--text)", 
            fontSize: "13px", 
            lineHeight: "1.45", 
            whiteSpace: "pre-wrap",
            wordBreak: "break-word"
          }}>
            {item.body}
          </p>
        </div>
      </div>
    );
  }

  if (item.kind === "assistant" && (item.meta === "intermediate" || item.meta === "streaming")) {
    return (
      <div 
        style={{ 
          maxWidth: "760px",
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
          {"meta" in item && item.meta ? <span>{item.meta}</span> : null}
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
