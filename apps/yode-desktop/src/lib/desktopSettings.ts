import { invoke } from "@tauri-apps/api/core";
import { recordFromUnknown } from "./jsonUtils";
import {
  storageReadRaw,
  storageReadString,
  storageRemoveItem,
  storageWriteJson,
  storageWriteString
} from "./storageAdapter";

export type GeneralSettings = {
  bottomPanel: boolean;
  suggestedPrompts: boolean;
  contextUsage: boolean;
  requireOptEnter: boolean;
  followUpBehavior: string;
  codeReviewPolicy: string;
  completionNotification: string;
  permissionNotification: boolean;
  questionNotification: boolean;
};

export type GeneralSettingsPayload = GeneralSettings & {
  workMode: string;
  defaultFilePermission: boolean;
  autoReview: boolean;
  fullAccess: boolean;
  openDestination: string;
  showInMenuBar: boolean;
  terminalLocation: string;
  preventSleep: boolean;
};

export const GENERAL_SETTINGS_CHANGE_EVENT = "yode-general-settings-change";

export type GeneralSettingsChangeDetail = {
  key: string;
  value: string | boolean;
  payload: GeneralSettingsPayload;
};

export type ConfigurationSettings = {
  scope: string;
  approvalPolicy: string;
  sandboxSettings: string;
  exposeDependencies: boolean;
};

export type WorktreesSettings = {
  baseDir: string;
  autoDeleteOnSessionEnd: boolean;
  preserveUncommitted: boolean;
  cleanUnusedCache: boolean;
};

export type GitSettings = {
  branchPrefix: string;
  mergeMethod: string;
  showPrIcons: boolean;
  alwaysForcePush: boolean;
  createDraftPrs: boolean;
  autoDeleteWorktrees: boolean;
  autoDeleteLimit: number;
  commitInstructions: string;
  prInstructions: string;
};

export type BrowserSettings = {
  enabled: boolean;
  annotationScreenshots: string;
  approvalPolicy: string;
  blockedDomains: string[];
  allowedDomains: string[];
};

export type PersonalizationSettings = {
  personality: string;
  customInstructions: string;
  enableMemories: boolean;
  skipToolChats: boolean;
};

export type InstallStatus = "installed" | "uninstalled" | "installing";

export type ComputerUseSettings = {
  anyAppStatus: InstallStatus;
  chromeStatus: InstallStatus;
  allowedApps: string[];
};

export type HookEntry = {
  name: string;
  events: string[];
  command: string;
  timeoutSecs: number;
  canBlock: boolean;
  disabled: boolean;
  toolFilter?: string[];
};

export type HooksSettings = {
  enabled: boolean;
  hooks: HookEntry[];
};

export type McpTransport = "stdio" | "sse" | "http" | "websocket";

export type McpEnvMeta = {
  key: string;
  hasValue: boolean;
  source: string;
};

export type McpEnvInput = {
  value?: string;
  clear?: boolean;
};

export type McpServer = {
  name: string;
  transport: McpTransport;
  command?: string;
  args?: string[];
  url?: string;
  env?: McpEnvMeta[];
  disabled: boolean;
};

export type McpServerInput = Omit<McpServer, "env"> & {
  env?: Record<string, McpEnvInput>;
};

export const DEFAULT_GIT_SETTINGS: GitSettings = {
  branchPrefix: "yode/",
  mergeMethod: "merge",
  showPrIcons: true,
  alwaysForcePush: false,
  createDraftPrs: true,
  autoDeleteWorktrees: true,
  autoDeleteLimit: 15,
  commitInstructions: "",
  prInstructions: ""
};

export const DEFAULT_BROWSER_SETTINGS: BrowserSettings = {
  enabled: true,
  annotationScreenshots: "Always include",
  approvalPolicy: "Always ask",
  blockedDomains: [],
  allowedDomains: []
};

export const DEFAULT_PERSONALIZATION_SETTINGS: PersonalizationSettings = {
  personality: "Friendly",
  customInstructions: "",
  enableMemories: false,
  skipToolChats: false
};

export const DEFAULT_COMPUTER_USE_SETTINGS: ComputerUseSettings = {
  anyAppStatus: "uninstalled",
  chromeStatus: "uninstalled",
  allowedApps: []
};

export const DEFAULT_HOOKS: HookEntry[] = [
  {
    name: "Pre-commit check",
    events: ["pre_turn"],
    command: "npm run lint",
    timeoutSecs: 15,
    canBlock: true,
    disabled: false
  },
  {
    name: "Auto-format code",
    events: ["task_completed"],
    command: "cargo fmt",
    timeoutSecs: 10,
    canBlock: false,
    disabled: false
  }
];

export const DEFAULT_HOOKS_SETTINGS: HooksSettings = {
  enabled: true,
  hooks: DEFAULT_HOOKS
};

export const DEFAULT_MCP_SERVERS: McpServer[] = [
  {
    name: "node_repl",
    transport: "stdio",
    command: "node",
    args: [],
    env: [],
    disabled: false
  }
];

const CONFIGURATION_STORAGE_KEYS = {
  scope: "yode-config-scope",
  approvalPolicy: "yode-config-approval",
  sandboxSettings: "yode-config-sandbox",
  exposeDependencies: "yode-expose-deps"
} as const;

const WORKTREES_STORAGE_KEYS = {
  baseDir: "yode-worktrees-base-dir",
  autoDeleteOnSessionEnd: "yode-worktrees-auto-delete-session-end",
  preserveUncommitted: "yode-worktrees-preserve-uncommitted",
  cleanUnusedCache: "yode-worktrees-clean-unused-cache"
} as const;

const GIT_STORAGE_KEYS = {
  branchPrefix: "yode-git-branch-prefix",
  mergeMethod: "yode-git-merge-method",
  showPrIcons: "yode-git-show-pr-icons",
  alwaysForcePush: "yode-git-always-force-push",
  createDraftPrs: "yode-git-create-draft-prs",
  autoDeleteWorktrees: "yode-git-auto-delete-worktrees",
  autoDeleteLimit: "yode-git-auto-delete-limit",
  commitInstructions: "yode-git-commit-instructions",
  prInstructions: "yode-git-pr-instructions"
} as const;

const BROWSER_STORAGE_KEYS = {
  enabled: "yode-browser-enabled",
  annotationScreenshots: "yode-browser-annotation-screenshots",
  approvalPolicy: "yode-browser-approval",
  blockedDomains: "yode-browser-blocked-domains",
  allowedDomains: "yode-browser-allowed-domains"
} as const;

const PERSONALIZATION_STORAGE_KEYS = {
  personality: "yode-personality",
  customInstructions: "yode-custom-instructions",
  enableMemories: "yode-enable-memories",
  skipToolChats: "yode-skip-tool-chats"
} as const;

const COMPUTER_USE_STORAGE_KEYS = {
  anyAppStatus: "yode-computer-use-anyapp",
  chromeStatus: "yode-computer-use-chrome",
  allowedApps: "yode-computer-use-allowed-apps"
} as const;

const HOOKS_STORAGE_KEYS = {
  enabled: "yode-hooks-enabled",
  hooks: "yode-hooks-list"
} as const;

const MCP_STORAGE_KEYS = {
  servers: "yode-mcp-servers"
} as const;

export function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

export type DesktopSettingsStatus = {
  loaded: boolean;
  path: string;
  error?: string | null;
  backupPath?: string | null;
};

export function isDesktopSettingsStatus(value: unknown): value is DesktopSettingsStatus {
  const record = recordFromUnknown(value);
  return Boolean(record && typeof record.loaded === "boolean" && typeof record.path === "string");
}

/**
 * 查询桌面设置文件加载状态。损坏 JSON、根节点非对象或不可读文件都会如实报告
 * `loaded: false` 与中文错误说明，绝不静默回退默认值。
 */
export async function loadDesktopSettingsStatus(): Promise<DesktopSettingsStatus> {
  if (isTauriRuntime()) {
    try {
      const status = await invoke<unknown>("desktop_settings_status_get");
      if (isDesktopSettingsStatus(status)) return status;
    } catch (err) {
      console.error(err);
    }
  }
  return { loaded: true, path: "" };
}

/**
 * 用户显式恢复损坏的设置文件：后端先备份原文件再生成新配置。
 * 仅当用户主动触发时调用。
 */
export async function restoreDesktopSettings(): Promise<DesktopSettingsStatus | null> {
  if (!isTauriRuntime()) return null;
  try {
    const status = await invoke<unknown>("desktop_settings_restore");
    return isDesktopSettingsStatus(status) ? status : null;
  } catch (err) {
    console.error(err);
    return null;
  }
}

export async function loadDesktopSetting<T>(key: string, fallback: T): Promise<T> {
  if (isTauriRuntime()) {
    try {
      const result = await invoke<{ key: string; value?: T | null }>("desktop_setting_get", { key });
      if (result.value !== undefined && result.value !== null) return result.value;
    } catch (err) {
      console.error(err);
    }
  }
  const raw = storageReadRaw(key);
  if (raw === null) return fallback;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return raw as T;
  }
}

export async function saveDesktopSetting<T>(key: string, value: T): Promise<void> {
  if (typeof value === "string") {
    storageWriteString(key, value);
  } else {
    storageWriteJson(key, value);
  }
  if (!isTauriRuntime()) return;
  try {
    await invoke("desktop_setting_set", { request: { key, value } });
  } catch (err) {
    console.error(err);
  }
}

export function loadGeneralSettings(): GeneralSettings {
  const payload = loadGeneralSettingsPayload();
  return {
    bottomPanel: payload.bottomPanel,
    suggestedPrompts: payload.suggestedPrompts,
    contextUsage: payload.contextUsage,
    requireOptEnter: payload.requireOptEnter,
    followUpBehavior: payload.followUpBehavior,
    codeReviewPolicy: payload.codeReviewPolicy,
    completionNotification: payload.completionNotification,
    permissionNotification: payload.permissionNotification,
    questionNotification: payload.questionNotification
  };
}

export function loadGeneralSettingsPayload(): GeneralSettingsPayload {
  const followUpBehavior = storageReadRaw("yode-follow-up-behavior");
  return {
    workMode: storageReadString("yode-work-mode", "coding"),
    defaultFilePermission: storageReadString("yode-def-perm", "true") !== "false",
    autoReview: storageReadString("yode-auto-review", "true") !== "false",
    // 旧版曾把完全信任写入 localStorage，造成 UI 与后端真实权限不一致。
    // 协议字段仅为兼容后端通用设置载荷保留；后端会以运行时有效模式覆盖它。
    fullAccess: false,
    openDestination: storageReadString("yode-open-dest", "VS Code"),
    showInMenuBar: storageReadString("yode-show-menu-bar", "true") !== "false",
    bottomPanel: storageReadString("yode-bottom-panel", "true") !== "false",
    terminalLocation: storageReadString("yode-term-loc", "bottom"),
    preventSleep: storageReadString("yode-prevent-sleep", "false") === "true",
    codeReviewPolicy: storageReadString("yode-code-review-policy", "inline"),
    suggestedPrompts: storageReadString("yode-suggested-prompts", "true") !== "false",
    contextUsage: storageReadString("yode-context-usage", "false") === "true",
    // 当前运行时不支持向进行中的 turn 注入消息；旧版 "steer" 设置必须诚实回退。
    followUpBehavior: followUpBehavior === "steer" ? "queue" : (followUpBehavior || "queue"),
    requireOptEnter: storageReadString("yode-require-opt-enter", "false") === "true",
    completionNotification: storageReadString("yode-completion-notif", "Only when unfocused"),
    permissionNotification: storageReadString("yode-perm-notif", "true") !== "false",
    questionNotification: storageReadString("yode-question-notif", "true") !== "false"
  };
}

export function loadConfigurationSettings(): ConfigurationSettings {
  return {
    scope: storageReadString(CONFIGURATION_STORAGE_KEYS.scope, "User config"),
    approvalPolicy: storageReadString(CONFIGURATION_STORAGE_KEYS.approvalPolicy, "On request"),
    sandboxSettings: storageReadString(CONFIGURATION_STORAGE_KEYS.sandboxSettings, "Read only"),
    exposeDependencies: storageReadString(CONFIGURATION_STORAGE_KEYS.exposeDependencies, "true") !== "false"
  };
}

export function saveConfigurationSettings(settings: ConfigurationSettings): void {
  storageWriteString(CONFIGURATION_STORAGE_KEYS.scope, settings.scope);
  storageWriteString(CONFIGURATION_STORAGE_KEYS.approvalPolicy, settings.approvalPolicy);
  storageWriteString(CONFIGURATION_STORAGE_KEYS.sandboxSettings, settings.sandboxSettings);
  storageWriteString(CONFIGURATION_STORAGE_KEYS.exposeDependencies, String(settings.exposeDependencies));
}

export function loadWorktreesSettings(): WorktreesSettings {
  return {
    baseDir: storageReadString(WORKTREES_STORAGE_KEYS.baseDir, "~/.yode/worktrees"),
    autoDeleteOnSessionEnd: storageReadString(WORKTREES_STORAGE_KEYS.autoDeleteOnSessionEnd, "true") !== "false",
    preserveUncommitted: storageReadString(WORKTREES_STORAGE_KEYS.preserveUncommitted, "true") !== "false",
    cleanUnusedCache: storageReadString(WORKTREES_STORAGE_KEYS.cleanUnusedCache, "false") === "true"
  };
}

export async function loadPersistedWorktreesSettings(fallback = loadWorktreesSettings()): Promise<WorktreesSettings> {
  return {
    baseDir: await loadDesktopSetting(WORKTREES_STORAGE_KEYS.baseDir, fallback.baseDir),
    autoDeleteOnSessionEnd: await loadDesktopSetting(
      WORKTREES_STORAGE_KEYS.autoDeleteOnSessionEnd,
      fallback.autoDeleteOnSessionEnd
    ),
    preserveUncommitted: await loadDesktopSetting(
      WORKTREES_STORAGE_KEYS.preserveUncommitted,
      fallback.preserveUncommitted
    ),
    cleanUnusedCache: await loadDesktopSetting(WORKTREES_STORAGE_KEYS.cleanUnusedCache, fallback.cleanUnusedCache)
  };
}

export function saveWorktreesSetting<K extends keyof WorktreesSettings>(
  key: K,
  value: WorktreesSettings[K]
): Promise<void> {
  return saveDesktopSetting(WORKTREES_STORAGE_KEYS[key], value);
}

export function loadGitSettings(): GitSettings {
  return {
    branchPrefix: storageReadString(GIT_STORAGE_KEYS.branchPrefix, DEFAULT_GIT_SETTINGS.branchPrefix),
    mergeMethod: storageReadString(GIT_STORAGE_KEYS.mergeMethod, DEFAULT_GIT_SETTINGS.mergeMethod),
    showPrIcons: storageReadString(GIT_STORAGE_KEYS.showPrIcons, "true") !== "false",
    alwaysForcePush: storageReadString(GIT_STORAGE_KEYS.alwaysForcePush, "false") === "true",
    createDraftPrs: storageReadString(GIT_STORAGE_KEYS.createDraftPrs, "true") !== "false",
    autoDeleteWorktrees: storageReadString(GIT_STORAGE_KEYS.autoDeleteWorktrees, "true") !== "false",
    autoDeleteLimit: Number(storageReadString(GIT_STORAGE_KEYS.autoDeleteLimit, String(DEFAULT_GIT_SETTINGS.autoDeleteLimit))),
    commitInstructions: storageReadString(GIT_STORAGE_KEYS.commitInstructions, DEFAULT_GIT_SETTINGS.commitInstructions),
    prInstructions: storageReadString(GIT_STORAGE_KEYS.prInstructions, DEFAULT_GIT_SETTINGS.prInstructions)
  };
}

export async function loadPersistedGitSettings(fallback = DEFAULT_GIT_SETTINGS): Promise<GitSettings> {
  return {
    branchPrefix: await loadDesktopSetting(GIT_STORAGE_KEYS.branchPrefix, fallback.branchPrefix),
    mergeMethod: await loadDesktopSetting(GIT_STORAGE_KEYS.mergeMethod, fallback.mergeMethod),
    showPrIcons: await loadDesktopSetting(GIT_STORAGE_KEYS.showPrIcons, fallback.showPrIcons),
    alwaysForcePush: await loadDesktopSetting(GIT_STORAGE_KEYS.alwaysForcePush, fallback.alwaysForcePush),
    createDraftPrs: await loadDesktopSetting(GIT_STORAGE_KEYS.createDraftPrs, fallback.createDraftPrs),
    autoDeleteWorktrees: await loadDesktopSetting(GIT_STORAGE_KEYS.autoDeleteWorktrees, fallback.autoDeleteWorktrees),
    autoDeleteLimit: await loadDesktopSetting(GIT_STORAGE_KEYS.autoDeleteLimit, fallback.autoDeleteLimit),
    commitInstructions: await loadDesktopSetting(GIT_STORAGE_KEYS.commitInstructions, fallback.commitInstructions),
    prInstructions: await loadDesktopSetting(GIT_STORAGE_KEYS.prInstructions, fallback.prInstructions)
  };
}

export function saveGitSettings(settings: GitSettings): void {
  storageWriteString(GIT_STORAGE_KEYS.branchPrefix, settings.branchPrefix);
  storageWriteString(GIT_STORAGE_KEYS.mergeMethod, settings.mergeMethod);
  storageWriteString(GIT_STORAGE_KEYS.showPrIcons, JSON.stringify(settings.showPrIcons));
  storageWriteString(GIT_STORAGE_KEYS.alwaysForcePush, JSON.stringify(settings.alwaysForcePush));
  storageWriteString(GIT_STORAGE_KEYS.createDraftPrs, JSON.stringify(settings.createDraftPrs));
  storageWriteString(GIT_STORAGE_KEYS.autoDeleteWorktrees, JSON.stringify(settings.autoDeleteWorktrees));
  storageWriteString(GIT_STORAGE_KEYS.autoDeleteLimit, JSON.stringify(settings.autoDeleteLimit));
  storageWriteString(GIT_STORAGE_KEYS.commitInstructions, settings.commitInstructions);
  storageWriteString(GIT_STORAGE_KEYS.prInstructions, settings.prInstructions);
}

function loadStoredStringArray(key: string): string[] {
  try {
    const raw = storageReadRaw(key);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}

export function loadBrowserSettings(): BrowserSettings {
  return {
    enabled: storageReadString(BROWSER_STORAGE_KEYS.enabled, "true") !== "false",
    annotationScreenshots:
      storageReadString(BROWSER_STORAGE_KEYS.annotationScreenshots, DEFAULT_BROWSER_SETTINGS.annotationScreenshots),
    approvalPolicy: storageReadString(BROWSER_STORAGE_KEYS.approvalPolicy, DEFAULT_BROWSER_SETTINGS.approvalPolicy),
    blockedDomains: loadStoredStringArray(BROWSER_STORAGE_KEYS.blockedDomains),
    allowedDomains: loadStoredStringArray(BROWSER_STORAGE_KEYS.allowedDomains)
  };
}

export async function loadPersistedBrowserSettings(fallback = DEFAULT_BROWSER_SETTINGS): Promise<BrowserSettings> {
  return {
    enabled: await loadDesktopSetting(BROWSER_STORAGE_KEYS.enabled, fallback.enabled),
    annotationScreenshots: await loadDesktopSetting(
      BROWSER_STORAGE_KEYS.annotationScreenshots,
      fallback.annotationScreenshots
    ),
    approvalPolicy: await loadDesktopSetting(BROWSER_STORAGE_KEYS.approvalPolicy, fallback.approvalPolicy),
    blockedDomains: await loadDesktopSetting(BROWSER_STORAGE_KEYS.blockedDomains, fallback.blockedDomains),
    allowedDomains: await loadDesktopSetting(BROWSER_STORAGE_KEYS.allowedDomains, fallback.allowedDomains)
  };
}

export function saveBrowserSettings(settings: BrowserSettings): void {
  storageWriteString(BROWSER_STORAGE_KEYS.enabled, JSON.stringify(settings.enabled));
  storageWriteString(BROWSER_STORAGE_KEYS.annotationScreenshots, settings.annotationScreenshots);
  storageWriteString(BROWSER_STORAGE_KEYS.approvalPolicy, settings.approvalPolicy);
  storageWriteString(BROWSER_STORAGE_KEYS.blockedDomains, JSON.stringify(settings.blockedDomains));
  storageWriteString(BROWSER_STORAGE_KEYS.allowedDomains, JSON.stringify(settings.allowedDomains));
}

export function loadPersonalizationSettings(): PersonalizationSettings {
  return {
    personality:
      storageReadString(PERSONALIZATION_STORAGE_KEYS.personality, DEFAULT_PERSONALIZATION_SETTINGS.personality),
    customInstructions:
      storageReadRaw(PERSONALIZATION_STORAGE_KEYS.customInstructions) ||
      DEFAULT_PERSONALIZATION_SETTINGS.customInstructions,
    enableMemories: storageReadString(PERSONALIZATION_STORAGE_KEYS.enableMemories, "false") === "true",
    skipToolChats: storageReadString(PERSONALIZATION_STORAGE_KEYS.skipToolChats, "false") === "true"
  };
}

export async function loadPersistedPersonalizationSettings(
  fallback = DEFAULT_PERSONALIZATION_SETTINGS
): Promise<PersonalizationSettings> {
  return {
    personality: await loadDesktopSetting(PERSONALIZATION_STORAGE_KEYS.personality, fallback.personality),
    customInstructions: await loadDesktopSetting(
      PERSONALIZATION_STORAGE_KEYS.customInstructions,
      fallback.customInstructions
    ),
    enableMemories: await loadDesktopSetting(PERSONALIZATION_STORAGE_KEYS.enableMemories, fallback.enableMemories),
    skipToolChats: await loadDesktopSetting(PERSONALIZATION_STORAGE_KEYS.skipToolChats, fallback.skipToolChats)
  };
}

export function savePersonalizationSettings(settings: PersonalizationSettings): void {
  storageWriteString(PERSONALIZATION_STORAGE_KEYS.personality, settings.personality);
  storageWriteString(PERSONALIZATION_STORAGE_KEYS.customInstructions, settings.customInstructions);
  storageWriteString(PERSONALIZATION_STORAGE_KEYS.enableMemories, String(settings.enableMemories));
  storageWriteString(PERSONALIZATION_STORAGE_KEYS.skipToolChats, String(settings.skipToolChats));
}

export function savePersonalizationSetting<K extends keyof PersonalizationSettings>(
  key: K,
  value: PersonalizationSettings[K]
): Promise<void> {
  return saveDesktopSetting(PERSONALIZATION_STORAGE_KEYS[key], value);
}

function normalizeInstallStatus(value: string | null): InstallStatus {
  return value === "installed" || value === "installing" || value === "uninstalled"
    ? value
    : DEFAULT_COMPUTER_USE_SETTINGS.anyAppStatus;
}

export function loadComputerUseSettings(): ComputerUseSettings {
  return {
    anyAppStatus: normalizeInstallStatus(storageReadRaw(COMPUTER_USE_STORAGE_KEYS.anyAppStatus)),
    chromeStatus: normalizeInstallStatus(storageReadRaw(COMPUTER_USE_STORAGE_KEYS.chromeStatus)),
    allowedApps: loadStoredStringArray(COMPUTER_USE_STORAGE_KEYS.allowedApps)
  };
}

export async function loadPersistedComputerUseSettings(
  fallback = DEFAULT_COMPUTER_USE_SETTINGS
): Promise<ComputerUseSettings> {
  return {
    anyAppStatus: await loadDesktopSetting(COMPUTER_USE_STORAGE_KEYS.anyAppStatus, fallback.anyAppStatus),
    chromeStatus: await loadDesktopSetting(COMPUTER_USE_STORAGE_KEYS.chromeStatus, fallback.chromeStatus),
    allowedApps: await loadDesktopSetting(COMPUTER_USE_STORAGE_KEYS.allowedApps, fallback.allowedApps)
  };
}

export function saveComputerUseSettings(settings: ComputerUseSettings): void {
  storageWriteString(COMPUTER_USE_STORAGE_KEYS.anyAppStatus, settings.anyAppStatus);
  storageWriteString(COMPUTER_USE_STORAGE_KEYS.chromeStatus, settings.chromeStatus);
  storageWriteString(COMPUTER_USE_STORAGE_KEYS.allowedApps, JSON.stringify(settings.allowedApps));
}

export function normalizeHookEntry(raw: unknown): HookEntry | null {
  const entry = recordFromUnknown(raw);
  if (!entry) return null;
  const name = String(entry.name || "").trim();
  const command = String(entry.command || "").trim();
  const events = Array.isArray(entry.events) ? entry.events.map(String).filter(Boolean) : [];
  if (!name || !command || events.length === 0) return null;
  const toolFilterRaw = entry.toolFilter ?? entry.tool_filter;
  const toolFilter = Array.isArray(toolFilterRaw) ? toolFilterRaw.map(String).filter(Boolean) : undefined;
  return {
    name,
    command,
    events,
    timeoutSecs: Number(entry.timeoutSecs ?? entry.timeout_secs) || 10,
    canBlock: Boolean(entry.canBlock ?? entry.can_block),
    disabled: Boolean(entry.disabled),
    toolFilter: toolFilter && toolFilter.length > 0 ? toolFilter : undefined
  };
}

export function normalizeHooks(list: unknown): HookEntry[] {
  if (!Array.isArray(list)) return [];
  return list.map(normalizeHookEntry).filter((hook): hook is HookEntry => hook !== null);
}

function loadStoredHooks(): HookEntry[] {
  try {
    const raw = storageReadRaw(HOOKS_STORAGE_KEYS.hooks);
    if (!raw) return DEFAULT_HOOKS;
    const hooks = normalizeHooks(JSON.parse(raw));
    return hooks.length > 0 ? hooks : DEFAULT_HOOKS;
  } catch {
    return DEFAULT_HOOKS;
  }
}

export function loadHooksSettings(): HooksSettings {
  return {
    enabled: storageReadString(HOOKS_STORAGE_KEYS.enabled, "true") !== "false",
    hooks: loadStoredHooks()
  };
}

export async function loadPersistedHooksSettings(fallback = DEFAULT_HOOKS_SETTINGS): Promise<HooksSettings> {
  const enabled = await loadDesktopSetting(HOOKS_STORAGE_KEYS.enabled, fallback.enabled);
  const hooks = normalizeHooks(await loadDesktopSetting(HOOKS_STORAGE_KEYS.hooks, fallback.hooks));
  return {
    enabled,
    hooks: hooks.length > 0 ? hooks : DEFAULT_HOOKS
  };
}

export function saveHooksSettings(settings: HooksSettings): void {
  storageWriteString(HOOKS_STORAGE_KEYS.enabled, JSON.stringify(settings.enabled));
  storageWriteString(HOOKS_STORAGE_KEYS.hooks, JSON.stringify(normalizeHooks(settings.hooks)));
}

export function isMcpTransport(value: string): value is McpTransport {
  return value === "stdio" || value === "sse" || value === "http" || value === "websocket";
}

function normalizeStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.map(String).filter(Boolean) : [];
}

function normalizeEnvMeta(value: unknown): McpEnvMeta[] {
  const record = recordFromUnknown(value);
  if (Array.isArray(value)) {
    return value
      .map((item) => {
        const entry = recordFromUnknown(item);
        if (!entry || typeof entry.key !== "string" || !entry.key.trim()) return null;
        return {
          key: entry.key.trim(),
          hasValue: Boolean(entry.hasValue),
          source: typeof entry.source === "string" ? entry.source : "配置文件"
        };
      })
      .filter((entry): entry is McpEnvMeta => entry !== null);
  }
  // 旧版本 localStorage 可能含有明文值；只迁移 key 元数据，并删除原始值。
  if (record) {
    return Object.keys(record)
      .filter((key) => key.trim())
      .map((key) => ({ key: key.trim(), hasValue: true, source: "旧配置" }));
  }
  return [];
}

export function normalizeMcpServer(raw: unknown): McpServer | null {
  const server = recordFromUnknown(raw);
  if (!server) return null;
  const name = String(server.name || "").trim();
  const transportRaw = String(server.transport || "stdio");
  if (!name || !isMcpTransport(transportRaw)) return null;

  const disabled = Boolean(server.disabled);
  if (transportRaw === "stdio") {
    const command = String(server.command || "").trim();
    if (!command) return null;
    return {
      name,
      transport: transportRaw,
      command,
      args: normalizeStringArray(server.args),
      env: normalizeEnvMeta(server.env),
      disabled
    };
  }

  const url = String(server.url || "").trim();
  if (!url) return null;
  return {
    name,
    transport: transportRaw,
    url,
    disabled
  };
}

export function normalizeMcpServers(list: unknown): McpServer[] {
  if (!Array.isArray(list)) return [];
  return list.map(normalizeMcpServer).filter((server): server is McpServer => server !== null);
}

export function loadMcpServers(): McpServer[] {
  try {
    const raw = storageReadRaw(MCP_STORAGE_KEYS.servers);
    // 该键属于旧版本，不能继续在浏览器存储中保留潜在密钥。
    storageRemoveItem(MCP_STORAGE_KEYS.servers);
    if (!raw) return DEFAULT_MCP_SERVERS;
    const servers = normalizeMcpServers(JSON.parse(raw));
    if (servers.length > 0) {
      return servers;
    }
    return DEFAULT_MCP_SERVERS;
  } catch {
    return DEFAULT_MCP_SERVERS;
  }
}

export async function loadPersistedMcpServers(fallback = DEFAULT_MCP_SERVERS): Promise<McpServer[]> {
  // MCP 的环境变量可能是令牌，禁止通过通用桌面设置或 localStorage 读取旧值。
  const normalized = normalizeMcpServers(fallback);
  return normalized.length > 0 ? normalized : DEFAULT_MCP_SERVERS;
}

export function saveMcpServers(servers: McpServer[]): void {
  // MCP 配置可能包含令牌。配置只由桌面后端持久化，WebView 不写入 localStorage。
  void servers;
  storageRemoveItem(MCP_STORAGE_KEYS.servers);
}

export function savePersistedMcpServers(servers: McpServer[]): Promise<void> {
  void servers;
  storageRemoveItem(MCP_STORAGE_KEYS.servers);
  return Promise.resolve();
}

export function saveGeneralSettingValue(key: string, value: string | boolean) {
  storageWriteString(key, String(value));
  window.dispatchEvent(
    new CustomEvent<GeneralSettingsChangeDetail>(GENERAL_SETTINGS_CHANGE_EVENT, {
      detail: { key, value, payload: loadGeneralSettingsPayload() }
    })
  );
}

export async function applyGeneralSettings(): Promise<void> {
  if (!isTauriRuntime()) return;
  try {
    await invoke("general_settings_apply", { settings: loadGeneralSettingsPayload() });
  } catch (err) {
    console.error(err);
  }
}

export function toggleBottomPanelSetting() {
  const next = storageReadRaw("yode-bottom-panel") === "false";
  saveGeneralSettingValue("yode-bottom-panel", next);
  return next;
}
