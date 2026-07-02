import { invoke } from "@tauri-apps/api/core";

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

export function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
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
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    const raw = localStorage.getItem(key);
    return (raw === null ? fallback : (raw as T));
  }
}

export async function saveDesktopSetting<T>(key: string, value: T): Promise<void> {
  localStorage.setItem(key, typeof value === "string" ? value : JSON.stringify(value));
  if (!isTauriRuntime()) return;
  try {
    await invoke("desktop_setting_set", { request: { key, value } });
  } catch (err) {
    console.error(err);
  }
}

export function loadGeneralSettings(): GeneralSettings {
  return {
    bottomPanel: localStorage.getItem("yode-bottom-panel") !== "false",
    suggestedPrompts: localStorage.getItem("yode-suggested-prompts") !== "false",
    contextUsage: localStorage.getItem("yode-context-usage") === "true",
    requireOptEnter: localStorage.getItem("yode-require-opt-enter") === "true",
    followUpBehavior: localStorage.getItem("yode-follow-up-behavior") || "queue",
    codeReviewPolicy: localStorage.getItem("yode-code-review-policy") || "inline",
    completionNotification: localStorage.getItem("yode-completion-notif") || "Only when unfocused",
    permissionNotification: localStorage.getItem("yode-perm-notif") !== "false",
    questionNotification: localStorage.getItem("yode-question-notif") !== "false"
  };
}

export function loadGeneralSettingsPayload(): GeneralSettingsPayload {
  return {
    workMode: localStorage.getItem("yode-work-mode") || "coding",
    defaultFilePermission: localStorage.getItem("yode-def-perm") !== "false",
    autoReview: localStorage.getItem("yode-auto-review") !== "false",
    fullAccess: localStorage.getItem("yode-full-access") !== "false",
    openDestination: localStorage.getItem("yode-open-dest") || "VS Code",
    showInMenuBar: localStorage.getItem("yode-show-menu-bar") !== "false",
    bottomPanel: localStorage.getItem("yode-bottom-panel") !== "false",
    terminalLocation: localStorage.getItem("yode-term-loc") || "bottom",
    preventSleep: localStorage.getItem("yode-prevent-sleep") === "true",
    codeReviewPolicy: localStorage.getItem("yode-code-review-policy") || "inline",
    suggestedPrompts: localStorage.getItem("yode-suggested-prompts") !== "false",
    contextUsage: localStorage.getItem("yode-context-usage") === "true",
    followUpBehavior: localStorage.getItem("yode-follow-up-behavior") || "queue",
    requireOptEnter: localStorage.getItem("yode-require-opt-enter") === "true",
    completionNotification: localStorage.getItem("yode-completion-notif") || "Only when unfocused",
    permissionNotification: localStorage.getItem("yode-perm-notif") !== "false",
    questionNotification: localStorage.getItem("yode-question-notif") !== "false"
  };
}

export function loadConfigurationSettings(): ConfigurationSettings {
  return {
    scope: localStorage.getItem(CONFIGURATION_STORAGE_KEYS.scope) || "User config",
    approvalPolicy: localStorage.getItem(CONFIGURATION_STORAGE_KEYS.approvalPolicy) || "On request",
    sandboxSettings: localStorage.getItem(CONFIGURATION_STORAGE_KEYS.sandboxSettings) || "Read only",
    exposeDependencies: localStorage.getItem(CONFIGURATION_STORAGE_KEYS.exposeDependencies) !== "false"
  };
}

export function saveConfigurationSettings(settings: ConfigurationSettings): void {
  localStorage.setItem(CONFIGURATION_STORAGE_KEYS.scope, settings.scope);
  localStorage.setItem(CONFIGURATION_STORAGE_KEYS.approvalPolicy, settings.approvalPolicy);
  localStorage.setItem(CONFIGURATION_STORAGE_KEYS.sandboxSettings, settings.sandboxSettings);
  localStorage.setItem(CONFIGURATION_STORAGE_KEYS.exposeDependencies, String(settings.exposeDependencies));
}

export function loadWorktreesSettings(): WorktreesSettings {
  return {
    baseDir: localStorage.getItem(WORKTREES_STORAGE_KEYS.baseDir) || "~/.yode/worktrees",
    autoDeleteOnSessionEnd: localStorage.getItem(WORKTREES_STORAGE_KEYS.autoDeleteOnSessionEnd) !== "false",
    preserveUncommitted: localStorage.getItem(WORKTREES_STORAGE_KEYS.preserveUncommitted) !== "false",
    cleanUnusedCache: localStorage.getItem(WORKTREES_STORAGE_KEYS.cleanUnusedCache) === "true"
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
    branchPrefix: localStorage.getItem(GIT_STORAGE_KEYS.branchPrefix) || DEFAULT_GIT_SETTINGS.branchPrefix,
    mergeMethod: localStorage.getItem(GIT_STORAGE_KEYS.mergeMethod) || DEFAULT_GIT_SETTINGS.mergeMethod,
    showPrIcons: localStorage.getItem(GIT_STORAGE_KEYS.showPrIcons) !== "false",
    alwaysForcePush: localStorage.getItem(GIT_STORAGE_KEYS.alwaysForcePush) === "true",
    createDraftPrs: localStorage.getItem(GIT_STORAGE_KEYS.createDraftPrs) !== "false",
    autoDeleteWorktrees: localStorage.getItem(GIT_STORAGE_KEYS.autoDeleteWorktrees) !== "false",
    autoDeleteLimit: Number(localStorage.getItem(GIT_STORAGE_KEYS.autoDeleteLimit) || DEFAULT_GIT_SETTINGS.autoDeleteLimit),
    commitInstructions: localStorage.getItem(GIT_STORAGE_KEYS.commitInstructions) || DEFAULT_GIT_SETTINGS.commitInstructions,
    prInstructions: localStorage.getItem(GIT_STORAGE_KEYS.prInstructions) || DEFAULT_GIT_SETTINGS.prInstructions
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
  localStorage.setItem(GIT_STORAGE_KEYS.branchPrefix, settings.branchPrefix);
  localStorage.setItem(GIT_STORAGE_KEYS.mergeMethod, settings.mergeMethod);
  localStorage.setItem(GIT_STORAGE_KEYS.showPrIcons, JSON.stringify(settings.showPrIcons));
  localStorage.setItem(GIT_STORAGE_KEYS.alwaysForcePush, JSON.stringify(settings.alwaysForcePush));
  localStorage.setItem(GIT_STORAGE_KEYS.createDraftPrs, JSON.stringify(settings.createDraftPrs));
  localStorage.setItem(GIT_STORAGE_KEYS.autoDeleteWorktrees, JSON.stringify(settings.autoDeleteWorktrees));
  localStorage.setItem(GIT_STORAGE_KEYS.autoDeleteLimit, JSON.stringify(settings.autoDeleteLimit));
  localStorage.setItem(GIT_STORAGE_KEYS.commitInstructions, settings.commitInstructions);
  localStorage.setItem(GIT_STORAGE_KEYS.prInstructions, settings.prInstructions);
}

function loadStoredStringArray(key: string): string[] {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}

export function loadBrowserSettings(): BrowserSettings {
  return {
    enabled: localStorage.getItem(BROWSER_STORAGE_KEYS.enabled) !== "false",
    annotationScreenshots:
      localStorage.getItem(BROWSER_STORAGE_KEYS.annotationScreenshots) || DEFAULT_BROWSER_SETTINGS.annotationScreenshots,
    approvalPolicy: localStorage.getItem(BROWSER_STORAGE_KEYS.approvalPolicy) || DEFAULT_BROWSER_SETTINGS.approvalPolicy,
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
  localStorage.setItem(BROWSER_STORAGE_KEYS.enabled, JSON.stringify(settings.enabled));
  localStorage.setItem(BROWSER_STORAGE_KEYS.annotationScreenshots, settings.annotationScreenshots);
  localStorage.setItem(BROWSER_STORAGE_KEYS.approvalPolicy, settings.approvalPolicy);
  localStorage.setItem(BROWSER_STORAGE_KEYS.blockedDomains, JSON.stringify(settings.blockedDomains));
  localStorage.setItem(BROWSER_STORAGE_KEYS.allowedDomains, JSON.stringify(settings.allowedDomains));
}

export function loadPersonalizationSettings(): PersonalizationSettings {
  return {
    personality:
      localStorage.getItem(PERSONALIZATION_STORAGE_KEYS.personality) || DEFAULT_PERSONALIZATION_SETTINGS.personality,
    customInstructions:
      localStorage.getItem(PERSONALIZATION_STORAGE_KEYS.customInstructions) ||
      DEFAULT_PERSONALIZATION_SETTINGS.customInstructions,
    enableMemories: localStorage.getItem(PERSONALIZATION_STORAGE_KEYS.enableMemories) === "true",
    skipToolChats: localStorage.getItem(PERSONALIZATION_STORAGE_KEYS.skipToolChats) === "true"
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
  localStorage.setItem(PERSONALIZATION_STORAGE_KEYS.personality, settings.personality);
  localStorage.setItem(PERSONALIZATION_STORAGE_KEYS.customInstructions, settings.customInstructions);
  localStorage.setItem(PERSONALIZATION_STORAGE_KEYS.enableMemories, String(settings.enableMemories));
  localStorage.setItem(PERSONALIZATION_STORAGE_KEYS.skipToolChats, String(settings.skipToolChats));
}

export function savePersonalizationSetting<K extends keyof PersonalizationSettings>(
  key: K,
  value: PersonalizationSettings[K]
): Promise<void> {
  return saveDesktopSetting(PERSONALIZATION_STORAGE_KEYS[key], value);
}

export function saveGeneralSettingValue(key: string, value: string | boolean) {
  localStorage.setItem(key, String(value));
  window.dispatchEvent(new CustomEvent("yode-general-settings-change", { detail: { key, value } }));
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
  const next = localStorage.getItem("yode-bottom-panel") === "false";
  saveGeneralSettingValue("yode-bottom-panel", next);
  return next;
}
