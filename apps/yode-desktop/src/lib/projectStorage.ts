import { SessionSummary } from "./desktopTypes";

export const PROJECT_ROOTS_STORAGE_KEY = "yode-project-roots";
export const PROJECT_ORDER_STORAGE_KEY = "yode-project-order";
export const SELECTED_PROJECT_ROOT_STORAGE_KEY = "yode-selected-project-root";
export const ARCHIVED_SESSION_IDS_STORAGE_KEY = "yode-archived-session-ids";
export const DELETED_SESSION_IDS_STORAGE_KEY = "yode-deleted-session-ids";
export const STANDALONE_PROJECT_SENTINEL = "__standalone__";

export function loadStoredProjectRoots(): string[] {
  return loadStoredProjectRootsByKey(PROJECT_ROOTS_STORAGE_KEY);
}

export function loadStoredProjectOrder(): string[] {
  return loadStoredProjectRootsByKey(PROJECT_ORDER_STORAGE_KEY);
}

export function loadStoredSelectedProjectRoot(): string | null | undefined {
  const raw = localStorage.getItem(SELECTED_PROJECT_ROOT_STORAGE_KEY);
  if (raw === null) return undefined;
  return raw === STANDALONE_PROJECT_SENTINEL ? null : raw;
}

export function normalizeProjectRoot(root: string | null | undefined) {
  const trimmed = root?.trim();
  return trimmed ? trimmed : null;
}

export function dedupeProjectRoots(roots: Array<string | null | undefined>) {
  const seen = new Set<string>();
  const unique: string[] = [];
  roots.forEach((root) => {
    const normalized = normalizeProjectRoot(root);
    if (!normalized || seen.has(normalized)) return;
    seen.add(normalized);
    unique.push(normalized);
  });
  return unique;
}

export function loadStoredStringArray(key: string): string[] {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((value): value is string => typeof value === "string");
  } catch {
    return [];
  }
}

export function visibleSessions(sessions: SessionSummary[]) {
  const hiddenIds = new Set([
    ...loadStoredStringArray(ARCHIVED_SESSION_IDS_STORAGE_KEY),
    ...loadStoredStringArray(DELETED_SESSION_IDS_STORAGE_KEY)
  ]);
  return sessions.filter((session) => !hiddenIds.has(session.id));
}

function loadStoredProjectRootsByKey(key: string): string[] {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return dedupeProjectRoots(parsed.filter((value): value is string => typeof value === "string"));
  } catch {
    return [];
  }
}
