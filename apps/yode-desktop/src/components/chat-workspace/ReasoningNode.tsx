import React, { useState, useEffect } from "react";
import { ChevronDown, ChevronRight, CircleDot, Brain } from "lucide-react";

interface ReasoningNodeProps {
  item: {
    id: string;
    title: string;
    body: string;
    meta?: string;
  };
  appLang: string;
}

export function ReasoningNode({ item, appLang }: ReasoningNodeProps) {
  const isRunning = item.meta === "running";
  const title = item.title || (isRunning ? "思考中" : "已思考");
  const displayTitle = isRunning ? title.replace(/[.。…]+$/g, "") : title;
  
  const [isExpanded, setIsExpanded] = useState(false);

  useEffect(() => {
    setIsExpanded(false);
  }, [item.id]);

  return (
    <div className="reasoning-node">
      {item.body && (
        <div className="reasoning-node-stack">
          <div
            onClick={() => {
              setIsExpanded(!isExpanded);
            }}
            className={`reasoning-node-trigger ${isRunning ? "running" : "complete"}`}
          >
            {isExpanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            {isRunning ? (
              <CircleDot
                size={10}
                className="process-pulse-dot"
              />
            ) : (
              <Brain size={10} className="reasoning-complete-icon" />
            )}
            <span>{displayTitle}</span>
          </div>

          {isExpanded && (
            <div className="reasoning-node-body">
              {item.body}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
