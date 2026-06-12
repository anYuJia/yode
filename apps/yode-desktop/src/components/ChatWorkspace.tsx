import React, { useState, useRef, useMemo, useLayoutEffect, useEffect, useCallback } from "react";
import {
  CircleDot,
  ChevronRight,
  Bot,
  ChevronDown,
  Check,
  Clock3,
  Copy,
  AlertCircle,
  TerminalSquare
} from "lucide-react";
import hljs from "highlight.js/lib/core";
import { TimelineItem } from "../lib/mock";
import { getFileIcon, fileIconMeta } from "./FileIcon";
import {
  ActivityGroupNode,
  ActivityItemNode,
  EditSummaryNode
} from "./activity/ActivityGroupNode";
import {
  isIntermediateAssistantItem,
  compileInlineItems,
  splitTurnVisibleItems,
  turnStaticDurationSeconds,
  formatDurationZh,
  ConversationTurn
} from "./timelineUtils";
import { Composer } from "./Composer";
import { RunInspector } from "./RunInspector";
import { invoke } from "@tauri-apps/api/core";

export type PendingUserQuestion = {
  sessionId: string;
  turnId: string;
  question: string;
};

export interface UserQueryOption {
  label: string;
  description: string;
  preview?: string;
}

export interface UserQuestion {
  question: string;
  header: string;
  options: UserQueryOption[];
  multiSelect?: boolean;
}

export interface UserQuery {
  questions: UserQuestion[];
}

interface ChatWorkspaceProps {
  draft: string;
  timelineItems: TimelineItem[];
  onDraftChange: (value: string) => void;
  onSendMessage: () => void;
  inspectorOpen: boolean;
  isProcessing: boolean;
  onCancelMessage: () => void;
  permissionMode: string;
  onPermissionModeChange: (mode: string) => void;
  onPermissionResolved: (id: string) => void;
  appLang: string;
  projectOptions: Array<{ label: string; root: string | null }>;
  selectedProjectRoot: string | null;
  onProjectRootChange: (root: string | null) => void;
  onAddProject: () => Promise<void>;
  currentProvider: string;
  currentModel: string;
  onModelChange: (model: string) => void;
  pendingUserQuestion: PendingUserQuestion | null;
  onAskUserResolve: (answer: string) => void;
}

export function ChatWorkspace({
  draft,
  timelineItems,
  onDraftChange,
  onSendMessage,
  inspectorOpen,
  isProcessing,
  onCancelMessage,
  permissionMode,
  onPermissionModeChange,
  onPermissionResolved,
  appLang,
  projectOptions,
  selectedProjectRoot,
  onProjectRootChange,
  onAddProject,
  currentProvider,
  currentModel,
  onModelChange,
  pendingUserQuestion,
  onAskUserResolve
}: ChatWorkspaceProps) {
  const isStreaming = useMemo(() => {
    if (isProcessing) return true;

    const hasRunningTool = timelineItems.some(item => item.kind === "tool" && item.status === "running");
    if (hasRunningTool) return true;
    
    const lastItem = timelineItems[timelineItems.length - 1];
    if (!lastItem) return false;
    if (lastItem.kind === "reasoning" && lastItem.meta === "running") {
      return true;
    }
    if (lastItem.kind === "assistant" && lastItem.meta !== "stream complete") {
      return true;
    }
    return false;
  }, [timelineItems, isProcessing]);

  const [expandedTurnIds, setExpandedTurnIds] = useState<string[]>([]);
  const previousStreamingRef = useRef(false);

  const turns = useMemo(() => {
    const list: ConversationTurn[] = [];

    let currentTurn: ConversationTurn = {
      id: "welcome",
      userItem: null,
      items: [],
      hasIntermediate: false
    };

    timelineItems.forEach((item) => {
      if (item.kind === "user") {
        if (currentTurn.userItem || currentTurn.items.length > 0) {
          list.push(currentTurn);
        }
        currentTurn = {
          id: item.id,
          userItem: item,
          items: [],
          hasIntermediate: false
        };
      } else {
        currentTurn.items.push(item);
        if (
          item.kind === "tool" ||
          item.kind === "reasoning" ||
          (item as any).kind === "process_note" ||
          (item.kind === "assistant" && isIntermediateAssistantItem(item))
        ) {
          currentTurn.hasIntermediate = true;
        }
      }
    });

    if (currentTurn.userItem || currentTurn.items.length > 0) {
      list.push(currentTurn);
    }

    return list;
  }, [timelineItems]);

  const activePermission = [...timelineItems]
    .reverse()
    .find((item): item is Extract<TimelineItem, { kind: "permission" }> => item.kind === "permission");

  const parsedStructuredQuery = useMemo(() => {
    if (!pendingUserQuestion) return null;
    try {
      const parsed = JSON.parse(pendingUserQuestion.question);
      if (parsed && Array.isArray(parsed.questions)) {
        return parsed as UserQuery;
      }
    } catch (e) {}
    return null;
  }, [pendingUserQuestion]);

  useEffect(() => {
    if (isStreaming && turns.length > 0) {
      const lastTurnId = turns[turns.length - 1].id;
      setExpandedTurnIds((prev) => {
        if (prev.includes(lastTurnId)) return prev;
        return [...prev, lastTurnId];
      });
    }
  }, [isStreaming, turns]);

  useEffect(() => {
    const wasStreaming = previousStreamingRef.current;
    previousStreamingRef.current = isStreaming;
    if (wasStreaming && !isStreaming && turns.length > 0) {
      const lastTurnId = turns[turns.length - 1].id;
      setExpandedTurnIds((prev) => prev.filter((id) => id !== lastTurnId));
    }
  }, [isStreaming, turns]);

  const timelinePanelRef = useRef<HTMLElement | null>(null);
  const shouldStickToBottomRef = useRef(true);
  const lastTimelineLengthRef = useRef(0);

  const scrollTimelineToBottom = (behavior: ScrollBehavior = "smooth") => {
    const panel = timelinePanelRef.current;
    if (!panel) return;
    panel.scrollTo({
      top: panel.scrollHeight,
      behavior
    });
  };

  const handleTimelineScroll = () => {
    const panel = timelinePanelRef.current;
    if (!panel) return;
    const distanceToBottom = panel.scrollHeight - panel.scrollTop - panel.clientHeight;
    shouldStickToBottomRef.current = distanceToBottom < 120;
  };

  useLayoutEffect(() => {
    if (!shouldStickToBottomRef.current) return;
    const itemAdded = timelineItems.length > lastTimelineLengthRef.current;
    lastTimelineLengthRef.current = timelineItems.length;
    const frame = window.requestAnimationFrame(() => {
      scrollTimelineToBottom(itemAdded && !isStreaming ? "smooth" : "auto");
    });
    return () => window.cancelAnimationFrame(frame);
  }, [timelineItems.length, isStreaming]);

  return (
    <div className={`chat-layout ${inspectorOpen ? "" : "inspector-collapsed"}`}>
      <div className="conversation-column">
        <section
          className="timeline-panel"
          aria-label="会话时间线"
          ref={timelinePanelRef}
          onScroll={handleTimelineScroll}
        >
          {turns.length === 0 ? (
            <div className="welcome-dashboard">
              <div className="welcome-logo">
                <Bot size={44} className="glowing-logo" style={{ color: "var(--accent)" }} />
              </div>
              <h1 className="welcome-title">{appLang === "zh" ? "今天想构建点什么？" : "What would you like to build today?"}</h1>
              <p className="welcome-subtitle">
                {appLang === "zh" 
                  ? "输入仓库任务以开始，我将帮助你分析、编写和调试代码。" 
                  : "Enter a repository task to start. I'll help you analyze, write, and debug code."}
              </p>
              
              <div className="welcome-cards">
                <button 
                  type="button"
                  className="welcome-card"
                  onClick={() => onDraftChange(appLang === "zh" ? "解释当前项目的主要架构 and 目录结构" : "Explain the main architecture and directory structure of this project")}
                >
                  <h3>🔍 {appLang === "zh" ? "分析项目" : "Analyze Project"}</h3>
                  <p>{appLang === "zh" ? "解释当前项目的主要架构和目录结构" : "Explain the main architecture and directory structure of this project"}</p>
                </button>
                <button 
                  type="button"
                  className="welcome-card"
                  onClick={() => onDraftChange(appLang === "zh" ? "帮我找出当前代码中可以优化性能的模块" : "Help me find modules in the current code that can be optimized for performance")}
                >
                  <h3>🛠️ {appLang === "zh" ? "代码优化" : "Code Optimization"}</h3>
                  <p>{appLang === "zh" ? "帮我找出当前代码中可以优化性能的模块" : "Help me find modules in the current code that can be optimized for performance"}</p>
                </button>
                <button 
                  type="button"
                  className="welcome-card"
                  onClick={() => onDraftChange(appLang === "zh" ? "为最近修改的 Rust 模块生成单元测试" : "Generate unit tests for the recently modified Rust modules")}
                >
                  <h3>📝 {appLang === "zh" ? "编写测试" : "Write Tests"}</h3>
                  <p>{appLang === "zh" ? "为最近修改的 Rust 模块生成单元测试" : "Generate unit tests for the recently modified Rust modules"}</p>
                </button>
              </div>
            </div>
          ) : (
            turns.map((turn, turnIndex) => {
              const isLastTurn = turnIndex === turns.length - 1;
              const isTurnActive = isStreaming && isLastTurn;
              const visibleItems = compileInlineItems(turn.items, isTurnActive, appLang);
              const { processItems, answerItems } = splitTurnVisibleItems(visibleItems);
              const hasProcessItems = processItems.length > 0;
              const isProcessExpanded = isTurnActive || expandedTurnIds.includes(turn.id);
              const durationSeconds = turnStaticDurationSeconds(turn);

              return (
                <React.Fragment key={turn.id}>
                  {turn.userItem && <TimelineNode item={turn.userItem} appLang={appLang} isTurnActive={isTurnActive} />}
                  {hasProcessItems ? (
                    <TurnProcessSummary
                      turnId={turn.id}
                      isActive={isTurnActive}
                      isExpanded={isProcessExpanded}
                      durationSeconds={durationSeconds}
                      processCount={processItems.length}
                      appLang={appLang}
                      onToggle={() => {
                        if (isTurnActive) return;
                        setExpandedTurnIds((prev) =>
                          prev.includes(turn.id)
                            ? prev.filter((id) => id !== turn.id)
                            : [...prev, turn.id]
                        );
                      }}
                    />
                  ) : null}
                  {(hasProcessItems && !isProcessExpanded ? [] : processItems).map((item) => (
                    <TimelineNode key={item.id} item={item} appLang={appLang} isTurnActive={isTurnActive} />
                  ))}
                  {answerItems.map((item) => (
                    <TimelineNode key={item.id} item={item} appLang={appLang} isTurnActive={isTurnActive} />
                  ))}
                </React.Fragment>
              );
            })
          )}
        </section>
        {parsedStructuredQuery ? (
          <div className="permission-dock" aria-label="用户提问确认">
            <AskUserActions
              query={parsedStructuredQuery}
              appLang={appLang}
              onResolve={onAskUserResolve}
            />
          </div>
        ) : activePermission ? (
          <div className="permission-dock" aria-label="执行确认">
            <PermissionActions
              item={activePermission}
              appLang={appLang}
              onResolved={() => onPermissionResolved(activePermission.id)}
            />
          </div>
        ) : null}
        <Composer
          draft={draft}
          onDraftChange={onDraftChange}
          onSendMessage={onSendMessage}
          isProcessing={isProcessing}
          onCancelMessage={onCancelMessage}
          permissionMode={permissionMode}
          onPermissionModeChange={onPermissionModeChange}
          appLang={appLang}
          projectOptions={projectOptions}
          selectedProjectRoot={selectedProjectRoot}
          onProjectRootChange={onProjectRootChange}
          onAddProject={onAddProject}
          currentProvider={currentProvider}
          currentModel={currentModel}
          onModelChange={onModelChange}
        />
      </div>
      <RunInspector
        isProcessing={isProcessing}
        permissionMode={permissionMode}
        timelineItems={timelineItems}
      />
    </div>
  );
}

function TurnProcessSummary({ turnId, isActive, isExpanded, onToggle, durationSeconds, processCount, appLang }: {
  turnId: string;
  isActive: boolean;
  isExpanded: boolean;
  onToggle: () => void;
  durationSeconds: number;
  processCount: number;
  appLang: string;
}) {
  const [elapsed, setElapsed] = useState(durationSeconds);
  const startRef = useRef<number | null>(null);
  const isZh = appLang === "zh";

  useEffect(() => {
    if (!isActive) {
      setElapsed(durationSeconds);
      return;
    }

    if (startRef.current === null) {
      startRef.current = Date.now() - durationSeconds * 1000;
    }
    const start = startRef.current;

    setElapsed(Math.floor((Date.now() - start) / 1000));

    const timer = setInterval(() => {
      setElapsed(Math.floor((Date.now() - start) / 1000));
    }, 1000);

    return () => clearInterval(timer);
  }, [turnId, isActive, durationSeconds]);

  const durationText = isZh ? formatDurationZh(elapsed) : `${elapsed}s`;
  const title = isActive
    ? isZh
      ? `正在处理，已用 ${durationText}`
      : `Working for ${durationText}`
    : isZh
      ? `任务完成，耗时 ${durationText}`
      : `Task finished in ${durationText}`;
  const detail = isActive
    ? isZh
      ? "过程正在展开"
      : "Process is visible"
    : isExpanded
      ? (isZh ? "收起过程" : "Collapse process")
      : (isZh ? `展开过程（${processCount} 项）` : `Show process (${processCount})`);

  return (
    <button
      onClick={onToggle}
      className={`turn-process-summary ${isActive ? "running" : "complete"} ${isExpanded ? "expanded" : "collapsed"}`}
      type="button"
      aria-expanded={isExpanded}
      aria-label={detail}
    >
      <span className="turn-process-summary-icon">
        {isActive ? <CircleDot size={10} className="glowing-logo" /> : <Check size={12} />}
      </span>
      <span className="turn-process-summary-main">{title}</span>
      <span className="turn-process-summary-detail">{detail}</span>
      <ChevronDown size={13} style={{
        opacity: 0.7,
        transform: isExpanded ? "none" : "rotate(-90deg)"
      }} />
    </button>
  );
}

function InlineToolGroup({ label, items, appLang }: { label: string; items: any[]; appLang: string }) {
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

function TimelineNode({ item, appLang, isTurnActive }: { item: TimelineItem; appLang: string; isTurnActive?: boolean }) {
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
        <MarkdownContent text={item.body} />
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
          <MarkdownContent text={item.body} />
        ) : "body" in item ? (
          <p>{(item as any).body}</p>
        ) : null}
      </div>
    </article>
  );
}

function CodeBlock({ text, lang }: { text: string; lang: string }) {
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

function MarkdownContent({ text }: { text: string }) {
  const blocks = useMemo(() => parseMarkdownBlocks(text), [text]);
  return (
    <div className="markdown-content">
      {blocks.map((block, index) => {
        if (block.type === "heading") {
          const Tag = `h${Math.min(block.level, 4)}` as keyof JSX.IntrinsicElements;
          return <Tag key={index}>{renderInlineMarkdown(block.text)}</Tag>;
        }
        if (block.type === "code") {
          return <CodeBlock key={index} text={block.text} lang={block.lang} />;
        }
        if (block.type === "list") {
          const ListTag = block.ordered ? "ol" : "ul";
          return (
            <ListTag key={index} style={{ paddingLeft: "20px", listStyleType: block.ordered ? "decimal" : "disc" }}>
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
              ))}
            </ListTag>
          );
        }
        if (block.type === "table") {
          return (
            <div key={index} className="markdown-table-wrapper" style={{ overflowX: "auto", margin: "12px 0" }}>
              <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "12px" }}>
                <thead>
                  <tr style={{ borderBottom: "2px solid var(--line)" }}>
                    {block.headers.map((h, i) => (
                      <th key={i} style={{ padding: "8px", textAlign: "left", fontWeight: "bold" }}>
                        {renderInlineMarkdown(h)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {block.rows.map((row, ri) => (
                    <tr key={ri} style={{ borderBottom: "1px solid var(--line-soft)" }}>
                      {row.map((cell, ci) => (
                        <td key={ci} style={{ padding: "8px" }}>
                          {renderInlineMarkdown(cell)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        }
        if (block.type === "divider") {
          return <hr key={index} style={{ border: "0", borderTop: "1px solid var(--line-soft)", margin: "16px 0" }} />;
        }
        return <p key={index}>{renderInlineMarkdown(block.text)}</p>;
      })}
    </div>
  );
}

type MarkdownBlock =
  | { type: "heading"; level: number; text: string }
  | { type: "code"; text: string; lang: string }
  | { type: "list"; ordered: boolean; items: string[] }
  | { type: "table"; headers: string[]; rows: string[][] }
  | { type: "divider" }
  | { type: "paragraph"; text: string };

function parseMarkdownBlocks(text: string): MarkdownBlock[] {
  const blocks: MarkdownBlock[] = [];
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  let paragraph: string[] = [];
  
  let currentListItems: string[] = [];
  let currentListOrdered = false;

  let tableRows: string[][] = [];
  let code: string[] | null = null;
  const flushParagraph = () => {
    if (paragraph.length > 0) {
      blocks.push({ type: "paragraph", text: paragraph.join(" ") });
      paragraph = [];
    }
  };
  const flushList = () => {
    if (currentListItems.length > 0) {
      blocks.push({ type: "list", ordered: currentListOrdered, items: currentListItems });
      currentListItems = [];
    }
  };
  const flushTable = () => {
    if (tableRows.length > 0) {
      if (tableRows.length >= 2 && tableRows[1].every(cell => /^:?-+:?$/.test(cell.trim()))) {
        const headers = tableRows[0];
        const rows = tableRows.slice(2);
        blocks.push({ type: "table", headers, rows });
      } else {
        for (const row of tableRows) {
          paragraph.push("|" + row.join("|") + "|");
        }
      }
      tableRows = [];
    }
  };
  let codeLang = "";
  for (const line of lines) {
    const fenceMatch = line.trim().match(/^```(.*)$/);
    if (fenceMatch) {
      if (code) {
        blocks.push({ type: "code", text: code.join("\n"), lang: codeLang });
        code = null;
        codeLang = "";
      } else {
        flushParagraph();
        flushList();
        flushTable();
        codeLang = fenceMatch[1].trim().toLowerCase();
        code = [];
      }
      continue;
    }

    if (code) {
      code.push(line);
      continue;
    }

    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) {
      flushParagraph();
      flushList();
      flushTable();
      blocks.push({ type: "heading", level: heading[1].length, text: heading[2].trim() });
      continue;
    }

    const unorderedMatch = line.match(/^\s*[-*]\s+(.+)$/);
    const orderedMatch = line.match(/^\s*(\d+)[.)]\s+(.+)$/);

    if (unorderedMatch) {
      flushParagraph();
      flushTable();
      if (currentListItems.length > 0 && currentListOrdered) {
        flushList();
      }
      currentListOrdered = false;
      currentListItems.push(unorderedMatch[1].trim());
      continue;
    }

    if (orderedMatch) {
      flushParagraph();
      flushTable();
      if (currentListItems.length > 0 && !currentListOrdered) {
        flushList();
      }
      currentListOrdered = true;
      currentListItems.push(orderedMatch[2].trim());
      continue;
    }

    const isDivider = /^(?:-{3,}|\*{3,}|_{3,})$/.test(line.trim());
    if (isDivider) {
      flushParagraph();
      flushList();
      flushTable();
      blocks.push({ type: "divider" });
      continue;
    }

    const isTableRow = line.trim().startsWith("|") && line.trim().endsWith("|");
    if (isTableRow) {
      flushParagraph();
      flushList();
      const cells = line.split("|").map(c => c.trim()).slice(1, -1);
      tableRows.push(cells);
      continue;
    }

    if (!line.trim()) {
      flushParagraph();
      flushList();
      flushTable();
      continue;
    }

    flushList();
    flushTable();
    paragraph.push(line.trim());
  }

  if (code) blocks.push({ type: "code", text: code.join("\n"), lang: codeLang });
  flushParagraph();
  flushList();
  flushTable();
  return blocks.length > 0 ? blocks : [{ type: "paragraph", text }];
}

function renderInlineMarkdown(text: string) {
  const parts = text.split(/(`[^`]+`|\*\*[^*]+\*\*)/g).filter(Boolean);
  return parts.map((part, index) => {
    if (part.startsWith("`") && part.endsWith("`")) {
      let codeText = part.slice(1, -1);
      
      if ((codeText.startsWith("'") && codeText.endsWith("'")) || (codeText.startsWith('"') && codeText.endsWith('"'))) {
        codeText = codeText.slice(1, -1);
      }
      
      const isFilename = /^[a-zA-Z0-9_\-./\\]+\.[a-zA-Z0-9]+$/.test(codeText) || codeText.startsWith(".") || codeText.includes("/.") || codeText.includes("\\.");
      
      if (isFilename) {
        const parts = codeText.split(/[/\\]/);
        const baseName = parts[parts.length - 1] || codeText;
        const meta = fileIconMeta(baseName);
        
        return (
          <code key={index} style={{ 
            display: "inline-flex", 
            alignItems: "center", 
            gap: "4px",
            verticalAlign: "middle",
            padding: "1px 6px"
          }}>
            {getFileIcon(baseName)}
            <span style={{ color: meta.color }}>{codeText}</span>
          </code>
        );
      }

      const isClassName = /^[A-Z][a-zA-Z0-9]+$/.test(codeText);
      if (isClassName) {
        return (
          <code key={index} style={{ 
            display: "inline-flex", 
            alignItems: "center", 
            gap: "4px",
            verticalAlign: "middle",
            padding: "1px 6px",
            border: "1px solid color-mix(in oklch, var(--accent), transparent 75%)",
            background: "color-mix(in oklch, var(--accent), transparent 94%)",
            color: "var(--accent)"
          }}>
            <span style={{ fontSize: "9px", opacity: 0.6, fontWeight: 700, fontFamily: "system-ui" }}>cls</span>
            <strong>{codeText}</strong>
          </code>
        );
      }

      const isFunction = /^[a-zA-Z_][a-zA-Z0-9_]*\s*\([^)]*\)$/.test(codeText);
      if (isFunction) {
        return (
          <code key={index} style={{ 
            display: "inline-flex", 
            alignItems: "center", 
            gap: "4px",
            verticalAlign: "middle",
            padding: "1px 6px",
            border: "1px solid color-mix(in oklch, var(--info), transparent 75%)",
            background: "color-mix(in oklch, var(--info), transparent 94%)",
            color: "var(--info)"
          }}>
            <span style={{ fontSize: "9px", opacity: 0.6, fontWeight: 700, fontFamily: "system-ui" }}>fn</span>
            <span>{codeText}</span>
          </code>
        );
      }

      const isVariable = /^[a-zA-Z_][a-zA-Z0-9_]*$/.test(codeText) && !/^[A-Z0-9_]+$/.test(codeText);
      if (isVariable) {
        return (
          <code key={index} style={{ 
            display: "inline-flex", 
            alignItems: "center", 
            gap: "4px",
            verticalAlign: "middle",
            padding: "1px 6px",
            border: "1px solid color-mix(in oklch, var(--warning), transparent 75%)",
            background: "color-mix(in oklch, var(--warning), transparent 94%)",
            color: "var(--warning)"
          }}>
            <span style={{ fontSize: "9px", opacity: 0.6, fontWeight: 700, fontFamily: "system-ui" }}>var</span>
            <span>{codeText}</span>
          </code>
        );
      }
      
      return <code key={index}>{codeText}</code>;
    }
    if (part.startsWith("**") && part.endsWith("**")) {
      return <strong key={index}>{part.slice(2, -2)}</strong>;
    }
    return <React.Fragment key={index}>{part}</React.Fragment>;
  });
}

function ProcessNoteNode({ note, appLang }: { note: Extract<TimelineItem, { kind: "process_note" }>; appLang: string }) {
  const isRunning = note.status === "running";
  const title = note.title;
  const body = note.body;

  if (!body && !title) return null;

  return (
    <div
      style={{
        maxWidth: "760px",
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
            color: isRunning ? "var(--accent)" : "var(--text-soft)",
            fontSize: "12.5px",
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
            color: "var(--text)",
            fontSize: "14.5px",
            lineHeight: 1.58,
            fontWeight: 500,
          }}
        >
          {renderInlineMarkdown(body)}
        </div>
      )}
    </div>
  );
}

function PermissionActions({
  item,
  appLang,
  onResolved
}: {
  item: Extract<TimelineItem, { kind: "permission" }>;
  appLang: string;
  onResolved?: () => void;
}) {
  const isZh = appLang === "zh";

  const options = [
    {
      id: "allow_once",
      label: isZh ? "允许本次执行" : "Yes, allow this time",
      description: isZh ? "仅允许本次执行" : "Only allow this execution"
    },
    {
      id: "always_allow",
      label: isZh ? "总是允许此命令" : "Yes, always allow this command",
      description: isZh ? "后续同类命令不再询问" : "Do not ask again for similar commands"
    },
    {
      id: "deny",
      label: isZh ? "拒绝并改用其他方式" : "No",
      description: isZh ? "告诉 agent 改用其他方式" : "Tell agent to use another way"
    }
  ] as const;

  const [selectedIndex, setSelectedIndex] = useState(0);
  const selectedOption = options[selectedIndex];
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const respond = (decision: (typeof options)[number]["id"]) => {
    onResolved?.();
    if (item.sessionId && item.turnId) {
      invoke("permission_respond", {
        sessionId: item.sessionId,
        turnId: item.turnId,
        allow: decision !== "deny",
        alwaysAllow: decision === "always_allow"
      }).catch(console.error);
    }
  };

  useEffect(() => {
    setSelectedIndex(0);
  }, [item.id]);

  useEffect(() => {
    optionRefs.current[selectedIndex]?.focus();
  }, [selectedIndex, item.id]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((index) => (index - 1 + options.length) % options.length);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((index) => (index + 1) % options.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        respond(selectedOption.id);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedOption.id, item.sessionId, item.turnId]);

  return (
    <div className="permission-prompt">
      <div className="permission-prompt-title">
        <TerminalSquare size={16} />
        <span>{isZh ? "允许运行此命令吗？" : "Allow running this command?"}</span>
      </div>
      <pre className="permission-command">{item.body || item.tool}</pre>
      <div className="permission-option-list">
        {options.map((option, index) => (
          <button
            className={`permission-option ${selectedIndex === index ? "selected" : ""}`}
            key={option.id}
            ref={(node) => {
              optionRefs.current[index] = node;
            }}
            onClick={() => {
              setSelectedIndex(index);
              respond(option.id);
            }}
            type="button"
            style={{ outline: "none", boxShadow: "none" }}
          >
            <kbd>{index + 1}</kbd>
            <span>{option.label}</span>
            <em>{option.description}</em>
          </button>
        ))}
      </div>
      <div className="permission-prompt-footer">
        <button className="permission-skip" onClick={() => respond("deny")} type="button" style={{ outline: "none", boxShadow: "none" }}>
          {isZh ? "跳过" : "Skip"}
        </button>
        <button className="permission-submit" onClick={() => respond(selectedOption.id)} type="button" style={{ outline: "none", boxShadow: "none" }}>
          {isZh ? "提交" : "Submit"}
          <span>↵</span>
        </button>
      </div>
    </div>
  );
}

function AskUserActions({
  query,
  appLang,
  onResolve
}: {
  query: UserQuery;
  appLang: string;
  onResolve: (answer: string) => void;
}) {
  const isZh = appLang === "zh";
  const question = query.questions[0];

  const [selectedIndex, setSelectedIndex] = useState(0);
  const [checkedIndices, setCheckedIndices] = useState<number[]>([]);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const handleToggle = (index: number) => {
    if (question.multiSelect) {
      setCheckedIndices((prev) =>
        prev.includes(index) ? prev.filter((i) => i !== index) : [...prev, index]
      );
    } else {
      setSelectedIndex(index);
    }
  };

  const submitAnswer = (idx?: number) => {
    const targetIdx = idx !== undefined ? idx : selectedIndex;
    if (question.multiSelect) {
      const selectedLabels = checkedIndices.map((i) => question.options[i].label);
      const answerObj = { [question.header || question.question]: selectedLabels };
      onResolve(JSON.stringify(answerObj));
    } else {
      const selectedOption = question.options[targetIdx];
      const answerObj = { [question.header || question.question]: selectedOption.label };
      onResolve(JSON.stringify(answerObj));
    }
  };

  useEffect(() => {
    setSelectedIndex(0);
    setCheckedIndices([]);
  }, [query]);

  useEffect(() => {
    optionRefs.current[selectedIndex]?.focus();
  }, [selectedIndex, query]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((index) => (index - 1 + question.options.length) % question.options.length);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((index) => (index + 1) % question.options.length);
      } else if (e.key === " ") {
        if (question.multiSelect) {
          e.preventDefault();
          handleToggle(selectedIndex);
        }
      } else if (e.key === "Enter") {
        e.preventDefault();
        submitAnswer();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedIndex, checkedIndices, query, question]);

  return (
    <div className="permission-prompt">
      <div className="permission-prompt-title">
        <CircleDot size={16} />
        <span>{question.header || (isZh ? "文件提问" : "Question")}</span>
      </div>
      <p style={{ margin: "9px 0 12px", fontSize: "13px", color: "var(--text)" }}>{question.question}</p>
      <div className="permission-option-list">
        {question.options.map((option, index) => {
          const isSelected = selectedIndex === index;
          const isChecked = question.multiSelect ? checkedIndices.includes(index) : isSelected;
          return (
            <button
              className={`permission-option ${isChecked ? "selected" : ""}`}
              key={option.label}
              ref={(node) => {
                optionRefs.current[index] = node;
              }}
              onClick={() => {
                if (question.multiSelect) {
                  handleToggle(index);
                } else {
                  submitAnswer(index);
                }
              }}
              type="button"
              style={{ outline: "none", boxShadow: "none", cursor: "pointer" }}
            >
              <kbd>{question.multiSelect ? (checkedIndices.includes(index) ? "✓" : " ") : index + 1}</kbd>
              <span>{option.label}</span>
              <em>{option.description}</em>
            </button>
          );
        })}
      </div>
      <div className="permission-prompt-footer">
        <button
          className="permission-submit"
          onClick={() => submitAnswer()}
          type="button"
          style={{ outline: "none", boxShadow: "none", cursor: "pointer" }}
        >
          {isZh ? "提交" : "Submit"}
          <span>↵</span>
        </button>
      </div>
    </div>
  );
}
