import React, { useState, useEffect } from "react";
import { ChevronDown, ChevronRight, CircleDot, Brain } from "lucide-react";
import { TimelineItem } from "../../lib/mock";
import { MarkdownContent } from "./MarkdownContent";

interface ReasoningNodeProps {
  item: any;
  appLang: string;
}

export function ReasoningNode({ item, appLang }: ReasoningNodeProps) {
  const isRunning = item.meta === "running";
  
  const [isExpanded, setIsExpanded] = useState(isRunning);
  const [hasManuallyToggled, setHasManuallyToggled] = useState(false);

  useEffect(() => {
    if (!hasManuallyToggled) {
      setIsExpanded(isRunning);
    }
  }, [isRunning, hasManuallyToggled]);

  return (
    <div
      style={{
        maxWidth: "760px",
        width: "100%",
        margin: "4px auto 8px",
        paddingLeft: "33px",
        display: "flex",
        flexDirection: "column",
        gap: "4px"
      }}
    >

      {/* 2. Collapsible Reasoning Link/Text */}
      {item.body && (
        <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
          {/* Simple Small Toggle Link */}
          <div
            onClick={() => {
              setHasManuallyToggled(true);
              setIsExpanded(!isExpanded);
            }}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: "6px",
              cursor: "pointer",
              userSelect: "none",
              fontSize: "11.5px",
              color: isRunning ? "var(--accent)" : "var(--text-muted)",
              opacity: 0.8,
              width: "fit-content"
            }}
          >
            {isExpanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            {isRunning ? (
              <CircleDot
                size={10}
                className="glowing-logo"
                style={{ animation: "pulse 1.5s infinite" }}
              />
            ) : (
              <Brain size={10} style={{ opacity: 0.8 }} />
            )}
            <span>{item.title || (isRunning ? "思考中..." : "已思考")}</span>
          </div>

          {/* Collapsible Content - Monospace plain text, no borders or background */}
          {isExpanded && (
            <div
              style={{
                paddingLeft: "18px",
                fontSize: "11.5px",
                lineHeight: "1.45",
                fontFamily: "var(--font-code)",
                whiteSpace: "pre-wrap",
                color: "var(--text-muted)",
                opacity: 0.85,
              }}
            >
              {item.body}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
