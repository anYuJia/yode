import { SessionSummary } from "./desktopTypes";

export const PROJECT_ROOTS_STORAGE_KEY = "yode-project-roots";
export const PROJECT_ORDER_STORAGE_KEY = "yode-project-order";
export const SELECTED_PROJECT_ROOT_STORAGE_KEY = "yode-selected-project-root";
export const ARCHIVED_SESSION_IDS_STORAGE_KEY = "yode-archived-session-ids";
export const ARCHIVED_CHATS_STORAGE_KEY = "yode-archived-chats";
export const DELETED_SESSION_IDS_STORAGE_KEY = "yode-deleted-session-ids";
export const STANDALONE_PROJECT_SENTINEL = "__standalone__";

export type ArchivedChatInfo = {
  id: string;
  title: string;
  date: string;
  project: string;
};

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

export function saveStoredStringArray(key: string, values: string[]) {
  localStorage.setItem(key, JSON.stringify(Array.from(new Set(values))));
}

export function addStoredStringArrayValue(key: string, value: string) {
  const values = loadStoredStringArray(key);
  if (!values.includes(value)) {
    saveStoredStringArray(key, [...values, value]);
  }
}

export function removeStoredStringArrayValue(key: string, value: string) {
  saveStoredStringArray(key, loadStoredStringArray(key).filter((item) => item !== value));
}

export function visibleSessions(sessions: SessionSummary[]) {
  const hiddenIds = new Set([
    ...loadStoredStringArray(ARCHIVED_SESSION_IDS_STORAGE_KEY),
    ...loadStoredStringArray(DELETED_SESSION_IDS_STORAGE_KEY)
  ]);
  return sessions.filter((session) => !hiddenIds.has(session.id));
}

export function isArchivedChat(value: unknown): value is ArchivedChatInfo {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.id === "string" &&
    typeof record.title === "string" &&
    typeof record.date === "string" &&
    typeof record.project === "string"
  );
}

export function loadStoredArchivedChats(): ArchivedChatInfo[] {
  try {
    const raw = localStorage.getItem(ARCHIVED_CHATS_STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter(isArchivedChat) : [];
  } catch {
    return [];
  }
}

export function saveStoredArchivedChats(chats: ArchivedChatInfo[]) {
  localStorage.setItem(ARCHIVED_CHATS_STORAGE_KEY, JSON.stringify(chats));
}

export function archiveSessionLocally(session: SessionSummary) {
  addStoredStringArrayValue(ARCHIVED_SESSION_IDS_STORAGE_KEY, session.id);
  const archivedChats = loadStoredArchivedChats();
  if (!archivedChats.some((chat) => chat.id === session.id)) {
    archivedChats.push({
      id: session.id,
      title: session.title,
      date: session.updatedAt,
      project: session.project || "default"
    });
    saveStoredArchivedChats(archivedChats);
  }
  return archivedChats;
}

export function unarchiveSessionLocally(sessionId: string) {
  removeStoredStringArrayValue(ARCHIVED_SESSION_IDS_STORAGE_KEY, sessionId);
  const updated = loadStoredArchivedChats().filter((chat) => chat.id !== sessionId);
  saveStoredArchivedChats(updated);
  return updated;
}

export function markArchivedSessionDeletedLocally(sessionId: string) {
  addStoredStringArrayValue(DELETED_SESSION_IDS_STORAGE_KEY, sessionId);
  removeStoredStringArrayValue(ARCHIVED_SESSION_IDS_STORAGE_KEY, sessionId);
  const updated = loadStoredArchivedChats().filter((chat) => chat.id !== sessionId);
  saveStoredArchivedChats(updated);
  return updated;
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
