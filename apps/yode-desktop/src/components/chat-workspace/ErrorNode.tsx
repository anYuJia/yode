import React, { useState } from "react";
import { AlertTriangle, Check, ChevronDown, Copy } from "lucide-react";
import { TimelineItem } from "../../lib/desktopTypes";

function summarizeError(text: string, appLang: string) {
  const clean = (text || "").replace(/\s+/g, " ").trim();
  if (!clean) return appLang === "zh" ? "执行过程中遇到错误。" : "The run failed.";
  const apiMatch = clean.match(/OpenAI API error \(([^)]+)\):\s*(.*?)(?:\s+LLM chat request failed|$)/i);
  if (apiMatch) {
    const detail = apiMatch[2]?.trim() || clean;
    return appLang === "zh" ? `上游接口返回 ${apiMatch[1]}：${detail}` : `Upstream API returned ${apiMatch[1]}: ${detail}`;
  }
  const requestMatch = clean.match(/Request failed after \d+ attempts?:\s*(.*)/i);
  if (requestMatch?.[1]) return requestMatch[1].trim();
  return clean.length > 180 ? `${clean.slice(0, 180)}...` : clean;
}

export function ErrorNode({ item, appLang }: { item: Extract<TimelineItem, { kind: "error" }>; appLang: string }) {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const isZh = appLang === "zh";
  const summary = summarizeError(item.body, appLang);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(item.body || "");
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <article className="error-node">
      <div className="error-node-shell">
        <div className="error-node-icon">
          <AlertTriangle size={15} />
        </div>
        <div className="error-node-content">
          <div className="error-node-header">
            <strong>{item.title || (isZh ? "请求失败" : "Request failed")}</strong>
            <button type="button" className="error-node-copy" onClick={copy} title={isZh ? "复制错误" : "Copy error"} aria-label={isZh ? "复制错误" : "Copy error"}>
              {copied ? <Check size={14} /> : <Copy size={14} />}
            </button>
          </div>
          <p>{summary}</p>
          {item.body && item.body !== summary ? (
            <>
              <button type="button" className="error-node-toggle" onClick={() => setExpanded((value) => !value)} aria-expanded={expanded}>
                <ChevronDown size={13} />
                <span>{expanded ? (isZh ? "收起详情" : "Hide details") : (isZh ? "查看详情" : "Show details")}</span>
              </button>
              {expanded ? <pre className="error-node-details">{item.body}</pre> : null}
            </>
          ) : null}
        </div>
      </div>
    </article>
  );
}
