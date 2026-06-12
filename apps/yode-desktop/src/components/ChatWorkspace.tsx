import React, { useState, useRef, useMemo, useLayoutEffect, useEffect } from "react";
import { Bot } from "lucide-react";
import { ImageAttachment, TimelineItem } from "../lib/mock";
import {
  isIntermediateAssistantItem,
  compileInlineItems,
  splitTurnVisibleItems,
  turnStaticDurationSeconds,
  ConversationTurn
} from "./timelineUtils";
import { Composer } from "./Composer";
import { RunInspector } from "./RunInspector";
import { TimelineNode } from "./chat-workspace/TimelineNode";
import { TurnProcessSummary } from "./chat-workspace/TurnProcessSummary";
import { PermissionActions } from "./chat-workspace/PermissionActions";
import { AskUserActions, UserQuery } from "./chat-workspace/AskUserActions";

export type PendingUserQuestion = {
  sessionId: string;
  turnId: string;
  question: string;
};

interface ChatWorkspaceProps {
  draft: string;
  timelineItems: TimelineItem[];
  onDraftChange: (value: string) => void;
  images: ImageAttachment[];
  onImagesChange: (images: ImageAttachment[]) => void;
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
  images,
  onImagesChange,
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
  const lastScrollTopRef = useRef(0);
  const touchStartYRef = useRef<number | null>(null);

  const scrollTimelineToBottom = (behavior: ScrollBehavior = "smooth") => {
    const panel = timelinePanelRef.current;
    if (!panel) return;
    panel.scrollTo({
      top: panel.scrollHeight,
      behavior
    });
  };

  const isNearTimelineBottom = (panel: HTMLElement, threshold = 120) => {
    const distanceToBottom = panel.scrollHeight - panel.scrollTop - panel.clientHeight;
    return distanceToBottom < threshold;
  };

  const handleTimelineScroll = () => {
    const panel = timelinePanelRef.current;
    if (!panel) return;
    const scrollTop = panel.scrollTop;
    const isScrollingUp = scrollTop < lastScrollTopRef.current - 1;
    if (isScrollingUp) {
      shouldStickToBottomRef.current = false;
    } else if (isNearTimelineBottom(panel)) {
      shouldStickToBottomRef.current = true;
    }
    lastScrollTopRef.current = scrollTop;
  };

  const handleTimelineWheel = (event: React.WheelEvent<HTMLElement>) => {
    const panel = timelinePanelRef.current;
    if (!panel) return;
    if (event.deltaY < 0) {
      shouldStickToBottomRef.current = false;
      return;
    }
    if (event.deltaY > 0 && isNearTimelineBottom(panel, 160)) {
      shouldStickToBottomRef.current = true;
    }
  };

  const handleTimelineTouchStart = (event: React.TouchEvent<HTMLElement>) => {
    touchStartYRef.current = event.touches[0]?.clientY ?? null;
  };

  const handleTimelineTouchMove = (event: React.TouchEvent<HTMLElement>) => {
    const panel = timelinePanelRef.current;
    const startY = touchStartYRef.current;
    const currentY = event.touches[0]?.clientY;
    if (!panel || startY == null || currentY == null) return;
    if (currentY > startY + 2) {
      shouldStickToBottomRef.current = false;
      return;
    }
    if (currentY < startY - 2 && isNearTimelineBottom(panel, 160)) {
      shouldStickToBottomRef.current = true;
    }
  };

  const timelineContentHash = useMemo(() => {
    return timelineItems.map(item => `${item.id}-${(item as any).body?.length || 0}`).join("|");
  }, [timelineItems]);

  useLayoutEffect(() => {
    const panel = timelinePanelRef.current;
    if (!panel) return;
    if (!shouldStickToBottomRef.current && !isNearTimelineBottom(panel, 80)) return;
    const itemAdded = timelineItems.length > lastTimelineLengthRef.current;
    lastTimelineLengthRef.current = timelineItems.length;
    const frame = window.requestAnimationFrame(() => {
      scrollTimelineToBottom(itemAdded && !isStreaming ? "smooth" : "auto");
    });
    return () => window.cancelAnimationFrame(frame);
  }, [timelineContentHash, isStreaming]);

  return (
    <div className={`chat-layout ${inspectorOpen ? "" : "inspector-collapsed"}`}>
      <div className="conversation-column">
        <section
          className="timeline-panel"
          aria-label="会话时间线"
          ref={timelinePanelRef}
          onScroll={handleTimelineScroll}
          onWheel={handleTimelineWheel}
          onTouchStart={handleTimelineTouchStart}
          onTouchMove={handleTimelineTouchMove}
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
          images={images}
          onImagesChange={onImagesChange}
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
