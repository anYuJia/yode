import React, { useState, useEffect } from "react";
import { CircleDot, ChevronDown, ChevronRight } from "lucide-react";
import { TimelineItem } from "../../lib/desktopTypes";
import { renderInlineMarkdown } from "./MarkdownContent";

export function ProcessNoteNode({ note, appLang }: { note: Extract<TimelineItem, { kind: "process_note" }>; appLang: string }) {
  const isRunning = note.status === "running";
  const title = note.title;
  const body = note.body;
  const isThinking = title && /思考|thinking|thought/i.test(title);
  const isActionNarrative = note.id.startsWith("action-narrative-");
  
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
      <div className="process-note-node">
        <div
          onClick={body ? () => {
            setHasManuallyToggled(true);
            setIsExpanded(!isExpanded);
          } : undefined}
          className={`process-note-title ${isRunning ? "running" : "complete"} ${body ? "interactive" : "static"} ${isExpanded && body ? "expanded" : ""}`}
        >
          {isRunning ? (
            <CircleDot size={10} className="process-pulse-dot" />
          ) : null}
          <span>{title}</span>
          {body ? (
            isExpanded ? (
              <ChevronDown size={12} className="process-chevron" />
            ) : (
              <ChevronRight size={12} className="process-chevron" />
            )
          ) : null}
        </div>
        {isExpanded && body && (
          <div className="process-note-detail process-note-detail-code">
            {body}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className={`process-note-node ${isActionNarrative ? "action-narrative" : ""}`}>
      {title && (
        <div
          className={`process-note-title ${isRunning ? "running" : "complete"} ${body ? "expanded" : ""}`}
        >
          {isRunning ? (
            <CircleDot size={10} className="process-pulse-dot" />
          ) : null}
          <span>{title}</span>
          {!isRunning ? <ChevronRight size={12} className="process-chevron" /> : null}
        </div>
      )}
      {body && (
        <div className={`process-note-body ${isRunning ? "running" : "complete"}`}>
          {isActionNarrative ? <CircleDot size={9} className="process-step-dot" /> : null}
          {renderInlineMarkdown(body)}
        </div>
      )}
    </div>
  );
}
