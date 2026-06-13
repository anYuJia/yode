import React, { useState, useRef, useMemo, useLayoutEffect, useEffect } from "react";
import { createPortal } from "react-dom";
import {
  Pin,
  Trash2,
  Folder,
  ChevronDown,
  Plus,
  MessageSquarePlus,
  Search,
  Code2,
  Workflow,
  Clock3,
  FolderPlus,
  Settings
} from "lucide-react";
import { SessionSummary } from "../lib/mock";
import { projectLabelFromPath } from "./timelineUtils";

export type ViewMode = "chat" | "settings";

interface SidebarProps {
  sessions: SessionSummary[];
  projectOptions: Array<{ label: string; root: string | null }>;
  activeSessionId: string | null;
  viewMode: ViewMode;
  onChangeView: (mode: ViewMode) => void;
  onCreateSession: (projectRoot?: string | null) => void;
  onSelectSession: (sessionId: string) => void;
  onAddProject: () => Promise<void>;
  onProjectReorder: (draggedRoot: string, targetRoot: string, placement?: "before" | "after") => void;
  onDeleteSession: (sessionId: string) => void;
}

export function Sidebar({
  sessions,
  projectOptions,
  activeSessionId,
  viewMode,
  onChangeView,
  onCreateSession,
  onSelectSession,
  onAddProject,
  onProjectReorder,
  onDeleteSession
}: SidebarProps) {
  const lang = localStorage.getItem("yode-language") || "zh";
  const isZh = lang === "zh";
  const t = (zhText: string, enText: string) => isZh ? zhText : enText;

  const [pinnedSessionIds, setPinnedSessionIds] = useState<string[]>(["s-1"]);
  const [deletingSessionId, setDeletingSessionId] = useState<string | null>(null);
  const [expandedProjectIds, setExpandedProjectIds] = useState<string[]>([]);
  const [draggingProjectId, setDraggingProjectId] = useState<string | null>(null);
  const [dragGhost, setDragGhost] = useState<{
    name: string;
    count: number;
    sessions: SessionSummary[];
    expanded: boolean;
    left: number;
    width: number;
    height: number;
    y: number;
  } | null>(null);
  
  // Hover information popover state
  const [hoveredSessionId, setHoveredSessionId] = useState<string | null>(null);
  const [hoverPosition, setHoverPosition] = useState<{ top: number; left: number } | null>(null);
  const hoverTimerRef = useRef<number | null>(null);
  const projectGroupsRef = useRef<Array<{ id: string; name: string; sessions: SessionSummary[] }>>([]);
  const projectNodeRefs = useRef(new Map<string, HTMLDivElement>());
  const projectFlipRectsRef = useRef(new Map<string, DOMRect>());
  const knownProjectIdsRef = useRef(new Set<string>());
  const dragStateRef = useRef<{
    id: string;
    name: string;
    count: number;
    sessions: SessionSummary[];
    expanded: boolean;
    left: number;
    width: number;
    height: number;
    offsetY: number;
    startY: number;
    hasMoved: boolean;
  } | null>(null);
  const suppressProjectClickRef = useRef(false);

  const handleMouseEnter = (sessionId: string, e: React.MouseEvent) => {
    if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
    
    const rect = e.currentTarget.getBoundingClientRect();
    const pos = {
      top: rect.top,
      left: 240
    };

    hoverTimerRef.current = window.setTimeout(() => {
      setHoveredSessionId(sessionId);
      setHoverPosition(pos);
    }, 600);
  };

  const handleMouseLeave = () => {
    if (hoverTimerRef.current) {
      window.clearTimeout(hoverTimerRef.current);
      hoverTimerRef.current = null;
    }
    setHoveredSessionId(null);
    setHoverPosition(null);
  };

  useEffect(() => {
    return () => {
      if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
    };
  }, []);

  useEffect(() => {
    const sessionIds = new Set(sessions.map((session) => session.id));

    if (hoveredSessionId && !sessionIds.has(hoveredSessionId)) {
      handleMouseLeave();
    }

    if (deletingSessionId && !sessionIds.has(deletingSessionId)) {
      setDeletingSessionId(null);
    }
  }, [sessions, hoveredSessionId, deletingSessionId]);

  const handleTogglePin = (sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setPinnedSessionIds(prev => 
      prev.includes(sessionId) 
        ? prev.filter(id => id !== sessionId) 
        : [...prev, sessionId]
    );
  };

  const handleDeleteClick = (sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setDeletingSessionId(sessionId);
  };

  const handleConfirmDelete = (sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    handleMouseLeave();
    onDeleteSession(sessionId);
    setDeletingSessionId(null);
  };

  const handleSessionMouseLeave = (sessionId: string) => {
    if (deletingSessionId === sessionId) {
      setDeletingSessionId(null);
    }
    handleMouseLeave();
  };

  const { projectGroups, standaloneSessions } = useMemo(() => {
    const groupMap = new Map<string, SessionSummary[]>();
    const standalone: SessionSummary[] = [];

    sessions.forEach((session) => {
      const projectRoot = session.projectRoot?.trim();
      if (!projectRoot) {
        standalone.push(session);
        return;
      }
      const existing = groupMap.get(projectRoot) ?? [];
      existing.push(session);
      groupMap.set(projectRoot, existing);
    });

    const sortSessions = (items: SessionSummary[]) =>
      [...items].sort((a, b) => {
        const pinDelta = Number(pinnedSessionIds.includes(b.id)) - Number(pinnedSessionIds.includes(a.id));
        return pinDelta || 0;
      });

    return {
      projectGroups: projectOptions
        .filter((option) => option.root)
        .map((option) => ({
          id: option.root!,
          name: option.label,
          sessions: sortSessions(groupMap.get(option.root!) ?? [])
        })),
      standaloneSessions: sortSessions(standalone)
    };
  }, [pinnedSessionIds, projectOptions, sessions]);

  projectGroupsRef.current = projectGroups;
  const projectLayoutKey = useMemo(
    () => projectGroups.map((group) => group.id).join("\n"),
    [projectGroups]
  );

  useLayoutEffect(() => {
    const previousRects = projectFlipRectsRef.current;
    const nextRects = new Map<string, DOMRect>();

    projectGroupsRef.current.forEach((group) => {
      const node = projectNodeRefs.current.get(group.id);
      if (!node) return;
      const nextRect = node.getBoundingClientRect();
      nextRects.set(group.id, nextRect);
      if (group.id === draggingProjectId) return;
      const previousRect = previousRects.get(group.id);
      if (!previousRect) return;
      const deltaY = previousRect.top - nextRect.top;
      if (Math.abs(deltaY) < 0.5) return;
      if (document.body.classList.contains("reduce-motion")) return;
      node.animate(
        [
          { transform: `translateY(${deltaY}px)` },
          { transform: "translateY(0)" }
        ],
        {
          duration: 260,
          easing: "cubic-bezier(0.16, 1, 0.3, 1)"
        }
      );
    });

    projectFlipRectsRef.current = nextRects;
  }, [projectLayoutKey, draggingProjectId]);

  useEffect(() => {
    const currentProjectGroups = projectGroupsRef.current;
    const nextKnownProjectIds = new Set(currentProjectGroups.map((group) => group.id));
    const newlyDiscoveredProjectIds = currentProjectGroups
      .filter((group) => !knownProjectIdsRef.current.has(group.id))
      .map((group) => group.id);
    knownProjectIdsRef.current = nextKnownProjectIds;

    setExpandedProjectIds((current) => {
      const kept = current.filter((id) => nextKnownProjectIds.has(id));
      const next = [
        ...kept,
        ...newlyDiscoveredProjectIds.filter((id) => !kept.includes(id))
      ];
      return next;
    });
  }, [projectLayoutKey]);

  const renderSessionItem = (session: SessionSummary) => {
    const isPinned = pinnedSessionIds.includes(session.id);
    const isDeleting = deletingSessionId === session.id;
    const isActive = session.id === activeSessionId;

    return (
      <div
        className={`session-item-wrapper ${isActive ? "active" : ""}`}
        key={session.id}
        onMouseEnter={(e) => handleMouseEnter(session.id, e)}
        onMouseLeave={() => handleSessionMouseLeave(session.id)}
        style={{ position: "relative" }}
      >
        <button
          className={`session-button ${isActive ? "active" : ""}`}
          onClick={() => onSelectSession(session.id)}
          type="button"
        >
          <span className="session-title">{session.title}</span>
          {!isDeleting ? <span className="session-time">{session.updatedAt}</span> : null}
        </button>

        {isDeleting ? (
          <div className="session-actions delete-confirm-overlay">
            <button
              onClick={(e) => handleConfirmDelete(session.id, e)}
              type="button"
              className="confirm-delete-btn"
            >
              {t("确认", "Confirm")}
            </button>
          </div>
        ) : (
          <div className="session-actions session-actions-overlay">
            <button
              onClick={(e) => handleTogglePin(session.id, e)}
              type="button"
              className="action-icon-btn"
              title={isPinned ? t("取消置顶", "Unpin") : t("置顶", "Pin")}
            >
              <Pin size={13} style={{ transform: isPinned ? "rotate(45deg)" : "none" }} />
            </button>
            <button
              onClick={(e) => handleDeleteClick(session.id, e)}
              type="button"
              className="action-icon-btn"
              title={t("删除", "Delete")}
            >
              <Trash2 size={13} />
            </button>
          </div>
        )}
      </div>
    );
  };

  const beginProjectPointerTracking = (
    group: { id: string; name: string; sessions: SessionSummary[] },
    event: React.PointerEvent<HTMLButtonElement>
  ) => {
    if (event.button !== 0) return;
    const groupNode = projectNodeRefs.current.get(group.id);
    const rect = (groupNode ?? event.currentTarget).getBoundingClientRect();
    const isExpandedAtStart = expandedProjectIds.includes(group.id);
    dragStateRef.current = {
      id: group.id,
      name: group.name,
      count: group.sessions.length,
      sessions: group.sessions,
      expanded: isExpandedAtStart,
      left: rect.left,
      width: rect.width,
      height: rect.height,
      offsetY: event.clientY - rect.top,
      startY: event.clientY,
      hasMoved: false
    };

    const handlePointerMove = (moveEvent: PointerEvent) => {
      const dragState = dragStateRef.current;
      if (!dragState) return;
      const moved = Math.abs(moveEvent.clientY - dragState.startY) > 4;
      if (!dragState.hasMoved) {
        if (!moved) return;
        dragState.hasMoved = true;
        suppressProjectClickRef.current = true;
        setDraggingProjectId(dragState.id);
        setDragGhost({
          name: dragState.name,
          count: dragState.count,
          sessions: dragState.sessions,
          expanded: dragState.expanded,
          left: dragState.left,
          width: dragState.width,
          height: dragState.height,
          y: moveEvent.clientY - dragState.offsetY
        });
      }

      moveEvent.preventDefault();
      setDragGhost((current) =>
        current ? { ...current, y: moveEvent.clientY - dragState.offsetY } : current
      );

      const groups = projectGroupsRef.current.filter((item) => item.id !== dragState.id);
      if (groups.length === 0) return;

      let targetId = groups[groups.length - 1].id;
      let placement: "before" | "after" = "after";
      for (const item of groups) {
        const node = projectNodeRefs.current.get(item.id);
        if (!node) continue;
        const itemRect = node.getBoundingClientRect();
        if (moveEvent.clientY < itemRect.top + itemRect.height / 2) {
          targetId = item.id;
          placement = "before";
          break;
        }
      }
      onProjectReorder(dragState.id, targetId, placement);
    };

    const finishPointerTracking = () => {
      dragStateRef.current = null;
      setDraggingProjectId(null);
      setDragGhost(null);
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", finishPointerTracking);
      window.removeEventListener("pointercancel", finishPointerTracking);
      window.setTimeout(() => {
        suppressProjectClickRef.current = false;
      }, 0);
    };

    window.addEventListener("pointermove", handlePointerMove, { passive: false });
    window.addEventListener("pointerup", finishPointerTracking);
    window.addEventListener("pointercancel", finishPointerTracking);
  };

  const renderProjectGroup = (group: { id: string; name: string; sessions: SessionSummary[] }) => {
    const expanded = expandedProjectIds.includes(group.id);
    const hasActiveSession = group.sessions.some((session) => session.id === activeSessionId);
    const isDragging = draggingProjectId === group.id;

    const style: React.CSSProperties = {
      position: "relative",
      zIndex: isDragging ? 10 : 1
    };

    return (
      <div
        className={`project-group ${hasActiveSession ? "active" : ""} ${isDragging ? "dragging" : ""}`}
        key={group.id}
        ref={(node) => {
          if (node) {
            projectNodeRefs.current.set(group.id, node);
          } else {
            projectNodeRefs.current.delete(group.id);
          }
        }}
        style={style}
      >
        <div className="project-header-wrapper" style={{ position: "relative" }}>
          <button
            className={`project-button ${hasActiveSession ? "active" : ""}`}
            onPointerDown={(event) => {
              beginProjectPointerTracking(group, event);
            }}
            onClick={(event) => {
              if (suppressProjectClickRef.current) {
                event.preventDefault();
                return;
              }
              setExpandedProjectIds((current) =>
                current.includes(group.id)
                  ? current.filter((id) => id !== group.id)
                  : [...current, group.id]
              );
            }}
            type="button"
          >
            <Folder size={16} />
            <span>
              {group.name}
              <em>{group.sessions.length}</em>
            </span>
            <ChevronDown className={expanded ? "expanded" : ""} size={15} />
          </button>
          <div className="project-actions-overlay">
            <button
              onClick={(e) => {
                e.stopPropagation();
                onCreateSession(group.id);
              }}
              type="button"
              className="action-icon-btn"
              title={t("新建对话", "New chat")}
            >
              <Plus size={13} />
            </button>
          </div>
        </div>
        <div
          className={`project-sessions-shell ${expanded ? "expanded" : "collapsed"}`}
          aria-hidden={!expanded}
        >
          <div className="project-sessions-inner">
            <div className="project-sessions">
              {group.sessions.map(renderSessionItem)}
            </div>
            {group.sessions.length === 0 ? (
              <div className="project-empty">{t("暂无会话", "No chats yet")}</div>
            ) : null}
          </div>
        </div>
      </div>
    );
  };

  const hoveredSession = hoveredSessionId
    ? sessions.find((session) => session.id === hoveredSessionId)
    : null;

  return (
    <aside className="sidebar" style={{ position: "relative" }}>
      <div className="brand-row" data-tauri-drag-region>
        <div className="brand-mark">Y</div>
        <div data-tauri-drag-region>
          <div className="brand-title" data-tauri-drag-region>Yode</div>
          <div className="brand-subtitle" data-tauri-drag-region>local agent runtime</div>
        </div>
      </div>

      <button className="primary-action" onClick={() => onCreateSession()} type="button">
        <MessageSquarePlus size={17} />
        {t("新对话", "New chat")}
      </button>

      <nav className="nav-block" aria-label="主导航">
        <NavButton icon={<Search size={16} />} label={t("搜索", "Search")} />
        <NavButton icon={<Code2 size={16} />} label={t("技能", "Skills")} />
        <NavButton icon={<Workflow size={16} />} label={t("插件", "Plugins")} />
        <NavButton icon={<Clock3 size={16} />} label={t("自动化", "Autopilot")} />
      </nav>

      <div className="sidebar-section sessions">
        <div className="section-head">
          <div className="section-label">{t("项目与对话", "Projects & Chats")}</div>
          <button className="section-action" type="button" onClick={() => void onAddProject()}>
            <FolderPlus size={14} />
            {t("添加项目", "Add project")}
          </button>
        </div>
        <div className="sessions-list">
          {projectGroups.map(renderProjectGroup)}
          <div className="standalone-group">
            <div className="standalone-label">{t("独立对话", "Standalone")}</div>
            {standaloneSessions.length > 0
              ? standaloneSessions.map(renderSessionItem)
              : <div className="standalone-empty">{t("暂无独立对话", "No standalone chats")}</div>}
          </div>
        </div>
      </div>

      {hoveredSession && hoverPosition && createPortal(
        <div
          className="session-popover"
          style={{
            position: "fixed",
            top: hoverPosition.top,
            left: hoverPosition.left,
            zIndex: 9999,
            width: "220px",
            background: "var(--panel-raised)",
            border: "1px solid var(--line)",
            borderRadius: "var(--radius)",
            padding: "10px",
            boxShadow: "var(--shadow-raised)",
            color: "var(--text)",
            pointerEvents: "none",
            animation: "fadeIn 0.15s ease-out"
          }}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
            <div style={{ fontSize: "12px", fontWeight: "700", color: "var(--accent)" }}>
              {hoveredSession.title}
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: "3px", fontSize: "10.5px", color: "var(--text-muted)" }}>
              <div>
                <span style={{ color: "var(--text-soft)" }}>{t("项目：", "Project: ")}</span>
                <code>{hoveredSession.project || (hoveredSession.projectRoot ? projectLabelFromPath(hoveredSession.projectRoot) : t("独立对话", "Standalone"))}</code>
              </div>
              <div>
                <span style={{ color: "var(--text-soft)" }}>{t("更新时间：", "Updated: ")}</span>
                {hoveredSession.updatedAt}
              </div>
              <div>
                <span style={{ color: "var(--text-soft)" }}>{t("会话 ID：", "Session ID: ")}</span>
                <span style={{ fontFamily: "var(--font-code)", opacity: 0.8 }}>{hoveredSession.id}</span>
              </div>
            </div>
          </div>
        </div>,
        document.body
      )}

      {dragGhost && createPortal(
        <div
          className={`project-drag-ghost ${dragGhost.expanded ? "expanded" : ""}`}
          style={{
            left: dragGhost.left,
            top: dragGhost.y,
            width: dragGhost.width,
            height: dragGhost.height
          }}
        >
          <div className="project-drag-ghost-head">
            <Folder size={16} />
            <span>
              {dragGhost.name}
              <em>{dragGhost.count}</em>
            </span>
          </div>
          {dragGhost.expanded ? (
            <div className="project-drag-ghost-sessions">
              {dragGhost.sessions.length > 0 ? (
                dragGhost.sessions.map((session) => (
                  <div className="project-drag-ghost-session" key={session.id}>
                    <span>{session.title}</span>
                    <em>{session.updatedAt}</em>
                  </div>
                ))
              ) : (
                <div className="project-drag-ghost-empty">{t("暂无会话", "No chats yet")}</div>
              )}
            </div>
          ) : null}
        </div>,
        document.body
      )}

      <div className="sidebar-footer">
        <button
          className={`footer-button ${viewMode === "settings" ? "active" : ""}`}
          onClick={() => onChangeView("settings")}
          type="button"
          title={t("设置", "Settings")}
        >
          <Settings size={17} />
          {t("设置", "Settings")}
        </button>
      </div>
    </aside>
  );
}

function NavButton({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <button className="nav-button" type="button">
      {icon}
      <span>{label}</span>
    </button>
  );
}
