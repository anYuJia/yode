import React, { useState, useEffect } from "react";
import { CircleDot, ChevronDown, ChevronRight } from "lucide-react";
import { TimelineItem } from "../../lib/mock";
import { renderInlineMarkdown } from "./MarkdownContent";

export function ProcessNoteNode({ note, appLang }: { note: Extract<TimelineItem, { kind: "process_note" }>; appLang: string }) {
  const isRunning = note.status === "running";
  const title = note.title;
  const body = note.body;
  const isThinking = title && /思考|thinking|thought/i.test(title);
  
  const [hasManuallyToggled, setHasManuallyToggled] = useState(false);
  const [isExpanded, setIsExpanded] = useState(isRunning);

  useEffect(() => {
    setHasManuallyToggled(false);
    setIsExpanded(isRunning);
  }, [note.id]);

  useEffect(() => {
    if (!hasManuallyToggled) {
      setIsExpanded(isRunning);
    }
  }, [isRunning, hasManuallyToggled]);

  if (!body && !title) return null;

  if (isThinking) {
    return (
      <div
        style={{
          maxWidth: "1064px",
          width: "100%",
          margin: "6px auto 10px",
          paddingLeft: "33px",
          color: "var(--text)",
        }}
      >
        <div
          onClick={body ? () => {
            setHasManuallyToggled(true);
            setIsExpanded(!isExpanded);
          } : undefined}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "6px",
            marginBottom: (isExpanded && body) ? "8px" : 0,
            color: isRunning ? "var(--process-accent)" : "var(--process-meta)",
            fontSize: "12.75px",
            fontWeight: 560,
            cursor: body ? "pointer" : "default",
            userSelect: "none"
          }}
        >
          {isRunning ? (
            <CircleDot size={10} className="glowing-logo" style={{ animation: "pulse 1.5s infinite" }} />
          ) : null}
          <span>{title}</span>
          {body ? (
            isExpanded ? (
              <ChevronDown size={12} style={{ opacity: 0.55 }} />
            ) : (
              <ChevronRight size={12} style={{ opacity: 0.55 }} />
            )
          ) : null}
        </div>
        {isExpanded && body && (
          <div
            style={{
              maxWidth: "72ch",
              color: "var(--process-text)",
              fontSize: "13px",
              lineHeight: 1.5,
              fontFamily: "var(--font-code)",
              padding: "8px 12px",
              background: "color-mix(in oklch, var(--field), transparent 0%)",
              borderRadius: "6px",
              border: "1px solid var(--line-soft)",
              whiteSpace: "pre-wrap",
              margin: "4px 0",
              overflowX: "auto"
            }}
          >
            {body}
          </div>
        )}
      </div>
    );
  }

  return (
    <div
      style={{
        maxWidth: "1064px",
        width: "100%",
        margin: "6px auto 10px",
        paddingLeft: "33px",
        color: "var(--text)",
      }}
    >
      {title && (
        <div
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "6px",
            marginBottom: body ? "8px" : 0,
            color: isRunning ? "var(--process-accent)" : "var(--process-meta)",
            fontSize: "12.75px",
            fontWeight: 560,
          }}
        >
          {isRunning ? (
            <CircleDot size={10} className="glowing-logo" />
          ) : null}
          <span>{title}</span>
          {!isRunning ? <ChevronRight size={12} style={{ opacity: 0.55 }} /> : null}
        </div>
      )}
      {body && (
        <div
          style={{
            maxWidth: "72ch",
            color: isRunning ? "var(--process-accent)" : "var(--process-text)",
            fontSize: "13px",
            lineHeight: 1.55,
            fontWeight: 450,
          }}
        >
          {renderInlineMarkdown(body)}
        </div>
      )}
    </div>
  );
}
