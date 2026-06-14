import { invoke } from "@tauri-apps/api/core";

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
