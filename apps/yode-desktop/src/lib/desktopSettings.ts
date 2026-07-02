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

const CONFIGURATION_STORAGE_KEYS = {
  scope: "yode-config-scope",
  approvalPolicy: "yode-config-approval",
  sandboxSettings: "yode-config-sandbox",
  exposeDependencies: "yode-expose-deps"
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
