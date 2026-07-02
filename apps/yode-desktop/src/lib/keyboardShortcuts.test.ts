import { afterEach, describe, expect, it, vi } from "vitest";

import {
  DEFAULT_SHORTCUT_BINDINGS,
  KEYBOARD_SHORTCUTS_CHANGE_EVENT,
  KEYBOARD_SHORTCUTS_STORAGE_KEY,
  saveShortcutBindings,
  shortcutBindingOverridesFromBindings,
  shortcutBindingsFromOverrides
} from "./keyboardShortcuts";

describe("keyboard shortcut helpers", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("normalizes stored shortcut overrides against known defaults", () => {
    const bindings = shortcutBindingsFromOverrides([
      { id: "archive", keys: ["Ctrl+Shift+A", 42] },
      { id: "unknown", keys: ["⌘U"] },
      { id: "newchat", keys: "bad" }
    ]);

    expect(bindings.find((binding) => binding.id === "archive")?.keys).toEqual(["Ctrl+Shift+A"]);
    expect(bindings.find((binding) => binding.id === "newchat")?.keys).toEqual(
      DEFAULT_SHORTCUT_BINDINGS.find((binding) => binding.id === "newchat")?.keys
    );
    expect(bindings.some((binding) => binding.id === "unknown")).toBe(false);
  });

  it("serializes bindings into compact override payloads", () => {
    expect(shortcutBindingOverridesFromBindings([
      { id: "archive", cmdZh: "归档对话", cmdEn: "Archive chat", descZh: "", descEn: "", keys: ["⇧⌘A"] }
    ])).toEqual([{ id: "archive", keys: ["⇧⌘A"] }]);
  });

  it("saves shortcut bindings and emits one shared change event", () => {
    const saved = new Map<string, string>();
    const dispatchEvent = vi.fn();
    vi.stubGlobal("localStorage", {
      setItem: (key: string, value: string) => saved.set(key, value)
    });
    vi.stubGlobal("window", { dispatchEvent });

    saveShortcutBindings([
      { id: "archive", cmdZh: "归档对话", cmdEn: "Archive chat", descZh: "", descEn: "", keys: ["⇧⌘A"] }
    ]);

    expect(JSON.parse(saved.get(KEYBOARD_SHORTCUTS_STORAGE_KEY) ?? "[]")).toEqual([{ id: "archive", keys: ["⇧⌘A"] }]);
    expect(dispatchEvent).toHaveBeenCalledTimes(1);
    expect(dispatchEvent.mock.calls[0]?.[0]).toMatchObject({ type: KEYBOARD_SHORTCUTS_CHANGE_EVENT });
  });
});
