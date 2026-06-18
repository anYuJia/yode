export const SIDEBAR_WIDTH_STORAGE_KEY = "yode-sidebar-width";
export const INSPECTOR_WIDTH_STORAGE_KEY = "yode-inspector-width";
export const TERMINAL_HEIGHT_STORAGE_KEY = "yode-terminal-height";

export const PANE_LIMITS = {
  sidebar: { min: 180, max: 420, fallback: 232 },
  inspector: { min: 220, max: 460, fallback: 260 },
  terminal: { min: 180, max: 520, fallback: 280 }
} as const;

export type PaneKind = keyof typeof PANE_LIMITS;

export type PaneDragState = {
  startX: number;
  startY: number;
  startSidebarWidth: number;
  startInspectorWidth: number;
  startTerminalHeight: number;
};

export function clampNumber(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function loadStoredNumber(key: string, fallback: number) {
  const raw = localStorage.getItem(key);
  if (!raw) return fallback;
  const parsed = Number(raw);
  return Number.isFinite(parsed) ? parsed : fallback;
}

export function loadInitialPaneSize(pane: PaneKind, storageKey: string) {
  const limits = PANE_LIMITS[pane];
  return clampNumber(loadStoredNumber(storageKey, limits.fallback), limits.min, limits.max);
}

export function computePaneDragSize(
  pane: PaneKind,
  drag: PaneDragState,
  pointer: { clientX: number; clientY: number },
  viewportHeight: number
) {
  if (pane === "sidebar") {
    return clampNumber(
      drag.startSidebarWidth + (pointer.clientX - drag.startX),
      PANE_LIMITS.sidebar.min,
      PANE_LIMITS.sidebar.max
    );
  }

  if (pane === "inspector") {
    return clampNumber(
      drag.startInspectorWidth - (pointer.clientX - drag.startX),
      PANE_LIMITS.inspector.min,
      PANE_LIMITS.inspector.max
    );
  }

  return clampNumber(
    drag.startTerminalHeight - (pointer.clientY - drag.startY),
    PANE_LIMITS.terminal.min,
    Math.floor(viewportHeight * 0.75)
  );
}

export function isPaneCollapsed(pane: PaneKind, size: number) {
  return size <= PANE_LIMITS[pane].min + 8;
}
