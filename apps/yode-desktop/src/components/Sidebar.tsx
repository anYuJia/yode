import React, { useState, useRef, useMemo, useLayoutEffect, useEffect } from "react";
import { createPortal } from "react-dom";
import {
  Archive,
  Pin,
  Folder,
  ChevronDown,
  Plus,
  MessageSquarePlus,
  Search,
  FolderPlus,
  Settings,
  X
} from "lucide-react";
import { SessionSummary, ViewMode } from "../lib/desktopTypes";
import { storageReadJson, storageWriteJson } from "../lib/storageAdapter";
import {
  PET_CHANGE_EVENT,
  loadAppLanguage,
  loadPetName,
  petFromChangeEvent
} from "../lib/appearanceSettings";
import { projectLabelFromPath } from "../lib/timelineUtils";

const PINNED_SESSIONS_STORAGE_KEY = "yode-pinned-sessions";

interface SidebarProps {
  isOpen: boolean;
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
  isOpen,
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
  const lang = loadAppLanguage();
  const isZh = lang === "zh";
  const t = (zhText: string, enText: string) => isZh ? zhText : enText;

  const [pinnedSessionIds, setPinnedSessionIds] = useState<string[]>(() => {
    const parsed = storageReadJson<unknown>(PINNED_SESSIONS_STORAGE_KEY, null);
    return Array.isArray(parsed) ? parsed.filter((id): id is string => typeof id === "string") : [];
  });
  const [deletingSessionId, setDeletingSessionId] = useState<string | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [expandedProjectIds, setExpandedProjectIds] = useState<string[]>([]);
  const [draggingProjectId, setDraggingProjectId] = useState<string | null>(null);
  const [projectReorderAnnouncement, setProjectReorderAnnouncement] = useState("");
  const [pet, setPet] = useState(() => loadPetName());
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
  const sidebarRef = useRef<HTMLElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
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

  useLayoutEffect(() => {
    const sidebar = sidebarRef.current;
    if (!sidebar) return;
    sidebar.toggleAttribute("inert", !isOpen);
  }, [isOpen]);

  const handleMouseEnter = (sessionId: string, e: React.MouseEvent) => {
    if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
    
    const rect = e.currentTarget.getBoundingClientRect();
    const pos = {
      top: Math.max(12, Math.min(rect.top, window.innerHeight - 156)),
      left: Math.max(12, Math.min(rect.right + 8, window.innerWidth - 232))
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
    if (!searchOpen) return;
    const frame = window.requestAnimationFrame(() => searchInputRef.current?.focus());
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (searchQuery) {
        setSearchQuery("");
      } else {
        setSearchOpen(false);
      }
    };
    window.addEventListener("keydown", handleEscape);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("keydown", handleEscape);
    };
  }, [searchOpen, searchQuery]);

  useEffect(() => {
    const handlePetChange = (event: Event) => {
      setPet(petFromChangeEvent(event));
    };
    window.addEventListener(PET_CHANGE_EVENT, handlePetChange);
    return () => window.removeEventListener(PET_CHANGE_EVENT, handlePetChange);
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
    setPinnedSessionIds((prev) => {
      const next = prev.includes(sessionId)
        ? prev.filter((id) => id !== sessionId)
        : [...prev, sessionId];
      // 置顶状态持久化，重启后保持排序
      storageWriteJson(PINNED_SESSIONS_STORAGE_KEY, next);
      return next;
    });
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

  const normalizedSearch = searchQuery.trim().toLocaleLowerCase();
  const visibleProjectGroups = useMemo(() => {
    if (!normalizedSearch) return projectGroups;
    return projectGroups
      .map((group) => {
        const projectMatches = group.name.toLocaleLowerCase().includes(normalizedSearch);
        return {
          ...group,
          sessions: projectMatches
            ? group.sessions
            : group.sessions.filter((session) =>
                session.title.toLocaleLowerCase().includes(normalizedSearch)
              )
        };
      })
      .filter((group) =>
        group.name.toLocaleLowerCase().includes(normalizedSearch) || group.sessions.length > 0
      );
  }, [normalizedSearch, projectGroups]);
  const visibleStandaloneSessions = useMemo(
    () =>
      normalizedSearch
        ? standaloneSessions.filter((session) =>
            session.title.toLocaleLowerCase().includes(normalizedSearch)
          )
        : standaloneSessions,
    [normalizedSearch, standaloneSessions]
  );
  const visibleSearchResultCount = useMemo(
    () =>
      visibleProjectGroups.reduce((total, group) => total + group.sessions.length, 0) +
      visibleStandaloneSessions.length,
    [visibleProjectGroups, visibleStandaloneSessions]
  );

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
          aria-current={isActive ? "page" : undefined}
          title={session.title}
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
              aria-label={t(`确认归档「${session.title}」`, `Confirm archiving “${session.title}”`)}
            >
              {t("确认", "Confirm")}
            </button>
            <button
              onClick={(event) => {
                event.stopPropagation();
                setDeletingSessionId(null);
              }}
              type="button"
              className="cancel-delete-btn"
              aria-label={t("取消归档", "Cancel archive")}
              title={t("取消", "Cancel")}
            >
              <X size={12} />
            </button>
          </div>
        ) : (
          <div className="session-actions session-actions-overlay">
            <button
              onClick={(e) => handleTogglePin(session.id, e)}
              type="button"
              className="action-icon-btn"
              title={isPinned ? t("取消置顶", "Unpin") : t("置顶", "Pin")}
              aria-label={isPinned ? t("取消置顶", "Unpin") : t("置顶", "Pin")}
            >
              <Pin size={13} style={{ transform: isPinned ? "rotate(45deg)" : "none" }} />
            </button>
            <button
              onClick={(e) => handleDeleteClick(session.id, e)}
              type="button"
              className="action-icon-btn"
              title={t("归档（可在归档记录中恢复）", "Archive (recoverable from archive)")}
              aria-label={t(`归档「${session.title}」`, `Archive “${session.title}”`)}
            >
              <Archive size={13} />
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
    const expanded = Boolean(normalizedSearch) || expandedProjectIds.includes(group.id);
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
            onKeyDown={(event) => {
              if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return;
              const currentIndex = projectGroups.findIndex((item) => item.id === group.id);
              const targetIndex = event.key === "ArrowUp" ? currentIndex - 1 : currentIndex + 1;
              const target = projectGroups[targetIndex];
              if (!target) return;

              event.preventDefault();
              onProjectReorder(
                group.id,
                target.id,
                event.key === "ArrowUp" ? "before" : "after"
              );
              setProjectReorderAnnouncement(
                t(
                  `已将项目 ${group.name} 移到第 ${targetIndex + 1} 位`,
                  `Moved project ${group.name} to position ${targetIndex + 1}`
                )
              );
            }}
            type="button"
            aria-expanded={expanded}
            aria-describedby="project-order-help"
            aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown"
            aria-label={t(
              `${expanded ? "收起" : "展开"}项目 ${group.name}，${group.sessions.length} 个对话`,
              `${expanded ? "Collapse" : "Expand"} project ${group.name}, ${group.sessions.length} chats`
            )}
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
              aria-label={t(`在 ${group.name} 中新建对话`, `Start a new chat in ${group.name}`)}
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
  const petMeta = {
    Yode: { mark: "Y", label: t("Yode 宠物", "Yode pet") },
    Cat: { mark: "C", label: t("猫猫", "Cat") },
    Dog: { mark: "D", label: t("狗狗", "Dog") }
  }[pet as "Yode" | "Cat" | "Dog"];

  return (
    <aside
      className="sidebar"
      id="app-sidebar"
      style={{ position: "relative" }}
      ref={sidebarRef}
      aria-hidden={!isOpen}
    >
      <div className="brand-row" data-tauri-drag-region>
        <div className="brand-mark" aria-hidden="true">
          <img src="/icon.svg" alt="" />
        </div>
        <div data-tauri-drag-region>
          <div className="brand-title" data-tauri-drag-region>Yode</div>
          <div className="brand-subtitle" data-tauri-drag-region>{t("本地智能编程助手", "Local coding agent")}</div>
        </div>
      </div>

      <button className="primary-action" onClick={() => onCreateSession()} type="button" aria-label={t("新建对话", "New chat")}>
        <MessageSquarePlus size={17} />
        {t("新对话", "New chat")}
      </button>

      <nav className="nav-block" aria-label="主导航">
        <NavButton
          icon={<Search size={16} />}
          label={t("搜索对话", "Search chats")}
          active={searchOpen}
          expanded={searchOpen}
          onClick={() => {
            setSearchOpen((current) => !current);
            if (searchOpen) setSearchQuery("");
          }}
        />
        {searchOpen ? (
          <div className="sidebar-search-field">
            <Search size={14} aria-hidden="true" />
            <input
              ref={searchInputRef}
              type="text"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder={t("按标题或项目搜索…", "Search titles or projects…")}
              aria-label={t("搜索对话", "Search chats")}
            />
            {searchQuery ? (
              <button
                type="button"
                onClick={() => setSearchQuery("")}
                aria-label={t("清空搜索", "Clear search")}
                title={t("清空搜索", "Clear search")}
              >
                <X size={12} />
              </button>
            ) : null}
          </div>
        ) : null}
      </nav>

      <div className="sidebar-section sessions">
        <div className="section-head">
          <div className="section-label">{t("项目与对话", "Projects & Chats")}</div>
          <button className="section-action" type="button" onClick={() => void onAddProject()} aria-label={t("添加项目", "Add project")}>
            <FolderPlus size={14} />
            {t("添加项目", "Add project")}
          </button>
        </div>
        <div className="sessions-list">
          <span className="sr-only" id="project-order-help">
            {t("按 Alt 加上、下方向键调整项目顺序", "Press Alt plus Up or Down Arrow to reorder the project")}
          </span>
          <span className="sr-only" role="status" aria-live="polite">
            {projectReorderAnnouncement}
          </span>
          {visibleProjectGroups.map(renderProjectGroup)}
          {(!normalizedSearch || visibleStandaloneSessions.length > 0) ? (
            <div className="standalone-group">
              <div className="standalone-label">{t("独立对话", "Standalone")}</div>
              {visibleStandaloneSessions.length > 0
                ? visibleStandaloneSessions.map(renderSessionItem)
                : <div className="standalone-empty">{t("暂无独立对话", "No standalone chats")}</div>}
            </div>
          ) : null}
          {normalizedSearch && visibleSearchResultCount === 0 ? (
            <div className="sidebar-search-empty" role="status">
              <Search size={18} />
              <strong>{t("没有匹配的对话", "No matching chats")}</strong>
              <span>{t("试试更短的关键词", "Try a shorter keyword")}</span>
            </div>
          ) : null}
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
        {petMeta ? (
          <div className="sidebar-pet" title={petMeta.label}>
            <span className="sidebar-pet-mark">{petMeta.mark}</span>
            <span className="sidebar-pet-label">{petMeta.label}</span>
          </div>
        ) : null}
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

function NavButton({
  icon,
  label,
  active,
  expanded,
  onClick
}: {
  icon: React.ReactNode;
  label: string;
  active?: boolean;
  expanded?: boolean;
  onClick?: () => void;
}) {
  return (
    <button
      className={`nav-button ${active ? "active" : ""}`}
      type="button"
      onClick={onClick}
      aria-expanded={expanded}
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}
