import React, { useState } from "react";
import { ChevronDown } from "lucide-react";

export function InlineToolGroup({ label, items, appLang }: { label: string; items: any[]; appLang: string }) {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div style={{
      maxWidth: "760px",
      width: "100%",
      margin: "4px auto 8px",
      paddingLeft: "33px",
      fontSize: "12px",
      color: "var(--text-soft)",
      userSelect: "none"
    }}>
      <div 
        onClick={() => setIsExpanded(!isExpanded)}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "6px",
          cursor: "pointer",
          transition: "color 0.15s ease",
        }}
        onMouseEnter={(e) => { e.currentTarget.style.color = "var(--text)"; }}
        onMouseLeave={(e) => { e.currentTarget.style.color = "var(--text-soft)"; }}
      >
        <span>{label}</span>
        <ChevronDown size={11} style={{
          opacity: 0.7,
          transform: isExpanded ? "none" : "rotate(-90deg)",
          transition: "transform 150ms ease"
        }} />
      </div>
      
      {isExpanded && items && items.length > 0 && (
        <div style={{
          marginTop: "6px",
          paddingLeft: "10px",
          borderLeft: "1.5px solid var(--line-soft)",
          display: "flex",
          flexDirection: "column",
          gap: "6px",
          fontSize: "11.5px"
        }}>
          {items.map((item, idx) => (
            <div key={idx} style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
              <div style={{ fontWeight: "600", color: "var(--text)" }}>{item.title}</div>
              {item.body && (
                <pre style={{
                  margin: "2px 0 0",
                  padding: "6px",
                  background: "color-mix(in oklch, var(--field), transparent 4%)",
                  borderRadius: "4px",
                  overflowX: "auto",
                  maxHeight: "80px",
                  whiteSpace: "pre-wrap",
                  fontFamily: "var(--font-code)",
                  fontSize: "10.5px",
                  color: "var(--text-muted)",
                  border: "1px solid var(--line-soft)"
                }}>
                  {item.body.length > 300 ? item.body.substring(0, 300) + "..." : item.body}
                </pre>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
