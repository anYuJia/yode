import React, { useState, useMemo, useCallback } from "react";
import { Check, Copy, AlertCircle } from "lucide-react";
import hljs from "highlight.js/lib/core";

export function CodeBlock({ text, lang }: { text: string; lang: string }) {
  const [copied, setCopied] = useState(false);

  const lines = text.split("\n");
  const truncateIndex = lines.findIndex(line => 
    line.includes("Output truncated by runtime guard") || 
    line.includes("truncated by runtime guard")
  );

  let cleanText = text;
  let isTruncated = false;
  if (truncateIndex !== -1) {
    isTruncated = true;
    cleanText = lines.slice(0, truncateIndex).join("\n");
  }

  const highlighted = useMemo(() => {
    try {
      if (lang && hljs.getLanguage(lang)) {
        return hljs.highlight(cleanText, { language: lang, ignoreIllegals: true }).value;
      }
      return hljs.highlightAuto(cleanText).value;
    } catch (e) {
      return cleanText
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
    }
  }, [cleanText, lang]);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(cleanText);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [cleanText]);

  return (
    <div className="code-block-container" style={{ 
      margin: "12px 0", 
      border: "1px solid var(--line-soft)", 
      borderRadius: "var(--radius)", 
      overflow: "hidden", 
      background: "color-mix(in oklch, var(--field), transparent 8%)",
      display: "flex",
      flexDirection: "column"
    }}>
      <div className="code-block-header" style={{ 
        display: "flex", 
        justifyContent: "space-between", 
        alignItems: "center", 
        padding: "6px 12px", 
        borderBottom: "1px solid var(--line-soft)", 
        background: "color-mix(in oklch, var(--field), transparent 4%)", 
        fontSize: "11px", 
        color: "var(--text-soft)",
        userSelect: "none"
      }}>
        <span style={{ fontFamily: "var(--font-code)", textTransform: "uppercase" }}>{lang || "code"}</span>
        <button onClick={handleCopy} style={{ 
          display: "flex", 
          alignItems: "center", 
          gap: "4px", 
          background: "transparent", 
          border: "none", 
          color: "var(--text-soft)", 
          cursor: "pointer", 
          padding: "2px 6px", 
          borderRadius: "4px" 
        }}>
          {copied ? <Check size={12} /> : <Copy size={12} />}
          <span>{copied ? "已复制" : "复制"}</span>
        </button>
      </div>
      <pre className="hljs" style={{ 
        margin: 0, 
        padding: "12px", 
        overflowX: "auto", 
        display: "block", 
        background: "transparent",
        whiteSpace: "pre",
        wordBreak: "normal",
        wordWrap: "normal"
      }}>
        <code dangerouslySetInnerHTML={{ __html: highlighted }} style={{
          border: 0,
          padding: 0,
          background: "transparent",
          fontFamily: "var(--font-code)",
          fontSize: "12px",
          lineHeight: "1.5"
        }} />
      </pre>
      {isTruncated && (
        <div style={{ 
          padding: "8px 12px", 
          borderTop: "1px solid var(--line-soft)", 
          background: "color-mix(in oklch, var(--warning), transparent 95%)", 
          color: "var(--text-soft)", 
          fontSize: "11.5px", 
          display: "flex", 
          alignItems: "center", 
          gap: "6px" 
        }}>
          <AlertCircle size={13} style={{ color: "var(--warning)" }} />
          <span>输出已被安全守护截断，可输入“继续”以获取完整内容。</span>
        </div>
      )}
    </div>
  );
}
