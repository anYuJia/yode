import { SessionSummary } from "./desktopTypes";

export const PROJECT_ROOTS_STORAGE_KEY = "yode-project-roots";
export const PROJECT_ORDER_STORAGE_KEY = "yode-project-order";
export const SELECTED_PROJECT_ROOT_STORAGE_KEY = "yode-selected-project-root";
export const ARCHIVED_SESSION_IDS_STORAGE_KEY = "yode-archived-session-ids";
export const ARCHIVED_CHATS_STORAGE_KEY = "yode-archived-chats";
export const DELETED_SESSION_IDS_STORAGE_KEY = "yode-deleted-session-ids";
export const ENVIRONMENT_PROJECTS_STORAGE_KEY = "yode-environments-projects";
export const STANDALONE_PROJECT_SENTINEL = "__standalone__";
export const PROJECT_ROOTS_CHANGED_EVENT = "yode-project-roots-changed";
export const SESSION_UNARCHIVED_EVENT = "yode-session-unarchived";
export const SESSION_DELETED_PERMANENTLY_EVENT = "yode-session-deleted-permanently";
export const SESSIONS_IMPORTED_EVENT = "yode-sessions-imported";

export type SessionIdEventDetail = {
  sessionId: string;
};

export type ArchivedChatInfo = {
  id: string;
  title: string;
  date: string;
  project: string;
};

export type ProjectEnvironment = {
  name: string;
  subtext?: string;
  path?: string;
  setupCommand?: string;
  execMode?: "host" | "docker" | "virtualenv";
  envVars?: Array<{ key: string; value: string }>;
};

export function projectLabelFromPath(path: string) {
  const trimmed = path.trim();
  if (!trimmed) return "项目";
  const parts = trimmed.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] || trimmed;
}

function ownerFromPath(path: string) {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts.length >= 2 ? parts[parts.length - 2] : undefined;
}

export function loadStoredProjectRoots(): string[] {
  return loadStoredProjectRootsByKey(PROJECT_ROOTS_STORAGE_KEY);
}

export function loadStoredProjectOrder(): string[] {
  return loadStoredProjectRootsByKey(PROJECT_ORDER_STORAGE_KEY);
}

export function loadRealProjectRoots() {
  const roots = loadStoredProjectRoots();
  const order = loadStoredProjectOrder();
  return dedupeProjectRoots([...order.filter((root) => roots.includes(root)), ...roots]);
}

export function saveRealProjectRoots(roots: string[]) {
  const deduped = dedupeProjectRoots(roots);
  saveStoredStringArray(PROJECT_ROOTS_STORAGE_KEY, deduped);
  saveStoredStringArray(PROJECT_ORDER_STORAGE_KEY, deduped);
  window.dispatchEvent(new CustomEvent(PROJECT_ROOTS_CHANGED_EVENT, { detail: deduped }));
}

export function dispatchSessionUnarchived(sessionId: string) {
  window.dispatchEvent(new CustomEvent(SESSION_UNARCHIVED_EVENT, { detail: { sessionId } }));
}

export function dispatchSessionDeletedPermanently(sessionId: string) {
  window.dispatchEvent(new CustomEvent(SESSION_DELETED_PERMANENTLY_EVENT, { detail: { sessionId } }));
}

export function detailFromSessionIdEvent(event: Event): SessionIdEventDetail | null {
  if (!(event instanceof CustomEvent)) return null;
  const detail = event.detail;
  if (!detail || typeof detail !== "object") return null;
  const sessionId = (detail as Record<string, unknown>).sessionId;
  return typeof sessionId === "string" && sessionId.length > 0 ? { sessionId } : null;
}

export function dispatchSessionsImported(sessions: SessionSummary[]) {
  window.dispatchEvent(new CustomEvent(SESSIONS_IMPORTED_EVENT, { detail: sessions }));
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

export function normalizeProjectEnvironment(project: ProjectEnvironment): ProjectEnvironment | null {
  const path = normalizeProjectRoot(project.path);
  const name = project.name?.trim() || (path ? projectLabelFromPath(path) : "");
  if (!path) return null;
  return {
    ...project,
    name,
    subtext: project.subtext?.trim() || ownerFromPath(path),
    path,
    execMode: project.execMode || "host",
    setupCommand: project.setupCommand || "",
    envVars: project.envVars || []
  };
}

export function mergeProjectEnvironments(
  savedProjects: ProjectEnvironment[],
  roots: string[]
): ProjectEnvironment[] {
  const byPath = new Map<string, ProjectEnvironment>();
  savedProjects.forEach((project) => {
    const normalized = normalizeProjectEnvironment(project);
    if (normalized?.path) byPath.set(normalized.path, normalized);
  });

  return roots.map((root) => {
    return byPath.get(root) ?? {
      name: projectLabelFromPath(root),
      subtext: ownerFromPath(root),
      path: root,
      execMode: "host" as const,
      setupCommand: "",
      envVars: []
    };
  });
}

export function normalizeProjectEnvironments(projects: ProjectEnvironment[]): ProjectEnvironment[] {
  return projects
    .map(normalizeProjectEnvironment)
    .filter((project): project is ProjectEnvironment => project !== null);
}

export function loadStoredProjectEnvironments(roots = loadRealProjectRoots()): ProjectEnvironment[] {
  try {
    const raw = localStorage.getItem(ENVIRONMENT_PROJECTS_STORAGE_KEY);
    if (!raw) return mergeProjectEnvironments([], roots);
    const parsed: unknown = JSON.parse(raw);
    return mergeProjectEnvironments(Array.isArray(parsed) ? (parsed as ProjectEnvironment[]) : [], roots);
  } catch {
    return mergeProjectEnvironments([], roots);
  }
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
