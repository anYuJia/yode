import React, { useMemo, useState } from "react";
import { Check, Plus, RotateCcw, Search, SlidersHorizontal, X } from "lucide-react";
import {
  DEFAULT_SHORTCUT_BINDINGS,
  KEYBOARD_SHORTCUTS_STORAGE_KEY,
  loadShortcutBindings,
  normalizeShortcutLabel,
  saveShortcutBindings,
  ShortcutBinding,
  shortcutBindingsFromOverrides,
  shortcutFromKeyboardEvent
} from "../../../lib/keyboardShortcuts";
import { loadDesktopSetting, saveDesktopSetting } from "../../../lib/desktopSettings";

export function KeyboardShortcutsSettings({ isZh, t }: { isZh: boolean; t: (zh: string, en: string) => string }) {
  const [searchQuery, setSearchQuery] = useState("");
  const [bindings, setBindings] = useState<ShortcutBinding[]>(() => loadShortcutBindings());
  const [recordingId, setRecordingId] = useState<string | null>(null);
  const [statusText, setStatusText] = useState("");

  const conflictMap = useMemo(() => {
    const map = new Map<string, string[]>();
    bindings.forEach((binding) => {
      binding.keys.forEach((key) => {
        const normalized = normalizeShortcutLabel(key);
        map.set(normalized, [...(map.get(normalized) ?? []), binding.id]);
      });
    });
    return map;
  }, [bindings]);

  React.useEffect(() => {
    void loadDesktopSetting<unknown>(KEYBOARD_SHORTCUTS_STORAGE_KEY, null).then((overrides) => {
      if (!overrides) return;
      const next = shortcutBindingsFromOverrides(overrides);
      setBindings(next);
      localStorage.setItem(KEYBOARD_SHORTCUTS_STORAGE_KEY, JSON.stringify(next.map((item) => ({ id: item.id, keys: item.keys }))));
      window.dispatchEvent(new Event("yode-keyboard-shortcuts-change"));
    });
  }, []);

  const filteredBindings = bindings.filter((binding) => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return true;
    return [
      binding.cmdZh,
      binding.cmdEn,
      binding.descZh,
      binding.descEn,
      ...binding.keys
    ].some((value) => value.toLowerCase().includes(query));
  });

  const persist = (next: ShortcutBinding[], message?: string) => {
    setBindings(next);
    saveShortcutBindings(next);
    void saveDesktopSetting(KEYBOARD_SHORTCUTS_STORAGE_KEY, next.map((binding) => ({ id: binding.id, keys: binding.keys })));
    if (message) setStatusText(message);
  };

  const handleDeleteBinding = (id: string, keyIdx: number) => {
    persist(
      bindings.map((binding) => {
        if (binding.id !== id) return binding;
        const nextKeys = [...binding.keys];
        nextKeys.splice(keyIdx, 1);
        return { ...binding, keys: nextKeys };
      }),
      t("快捷键已移除。", "Shortcut removed.")
    );
  };

  const handleRecordKey = (event: React.KeyboardEvent, id: string) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setRecordingId(null);
      return;
    }
    if (["Shift", "Control", "Meta", "Alt"].includes(event.key)) return;

    const key = shortcutFromKeyboardEvent(event);
    const normalized = normalizeShortcutLabel(key);
    persist(
      bindings.map((binding) => {
        if (binding.id !== id) return binding;
        const existing = new Set(binding.keys.map(normalizeShortcutLabel));
        if (existing.has(normalized)) return binding;
        return { ...binding, keys: [...binding.keys, key] };
      }),
      t("快捷键已保存。", "Shortcut saved.")
    );
    setRecordingId(null);
  };

  const resetToDefaults = () => {
    persist(
      DEFAULT_SHORTCUT_BINDINGS.map((binding) => ({ ...binding, keys: [...binding.keys] })),
      t("已恢复默认快捷键。", "Default shortcuts restored.")
    );
    setRecordingId(null);
  };

  const assignedCount = bindings.reduce((count, binding) => count + binding.keys.length, 0);
  const conflictCount = [...conflictMap.values()].filter((ids) => ids.length > 1).length;

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "14px" }}>
      <div style={{ display: "grid", gridTemplateColumns: "1fr auto auto", gap: "8px", alignItems: "center" }}>
        <div style={{ position: "relative", width: "100%" }}>
          <Search size={13} style={{ position: "absolute", left: "10px", top: "8px", color: "var(--text-soft)", opacity: 0.8 }} />
          <input
            type="text"
            placeholder={t("搜索命令或快捷键...", "Search commands or shortcuts...")}
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
            style={{
              width: "100%",
              height: "30px",
              background: "var(--field)",
              border: "1px solid var(--line-soft)",
              borderRadius: "var(--radius)",
              paddingLeft: "28px",
              paddingRight: "28px",
              fontSize: "12px",
              color: "var(--text)",
              outline: "none"
            }}
          />
          <SlidersHorizontal size={13} style={{ position: "absolute", right: "10px", top: "8px", color: "var(--text-soft)", opacity: 0.8 }} />
        </div>
        <span style={{ fontSize: "11px", color: conflictCount ? "oklch(67% 0.15 28)" : "var(--text-soft)", whiteSpace: "nowrap" }}>
          {conflictCount
            ? t(`${conflictCount} 个冲突`, `${conflictCount} conflicts`)
            : t(`${assignedCount} 个已分配`, `${assignedCount} assigned`)}
        </span>
        <button className="secondary-button" type="button" onClick={resetToDefaults} style={{ height: "30px", gap: "6px" }}>
          <RotateCcw size={13} />
          {t("恢复默认", "Reset")}
        </button>
      </div>

      <div className="theme-card" style={{ padding: "0 14px 10px" }}>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "minmax(260px, 1fr) minmax(220px, 260px)",
            gap: "20px",
            paddingBlock: "10px",
            borderBottom: "1px solid var(--line-soft)",
            fontSize: "11px",
            fontWeight: "700",
            color: "var(--text-soft)"
          }}
        >
          <span>{t("命令", "Command")}</span>
          <span>{t("快捷键", "Keybinding")}</span>
        </div>

        <div style={{ display: "flex", flexDirection: "column" }}>
          {filteredBindings.map((item) => (
            <div
              key={item.id}
              style={{
                display: "grid",
                gridTemplateColumns: "minmax(260px, 1fr) minmax(220px, 260px)",
                gap: "20px",
                paddingBlock: "12px",
                borderBottom: "1px solid var(--line-soft)",
                fontSize: "12px"
              }}
            >
              <div style={{ display: "flex", flexDirection: "column", gap: "3px", minWidth: 0 }}>
                <span style={{ fontWeight: "650", color: "var(--text)" }}>{t(item.cmdZh, item.cmdEn)}</span>
                <span style={{ fontSize: "11px", color: "var(--text-soft)", lineHeight: 1.35 }}>{t(item.descZh, item.descEn)}</span>
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "6px", justifyContent: "center", minWidth: 0 }}>
                {item.keys.length === 0 ? (
                  <span style={{ fontSize: "11px", color: "var(--text-soft)", opacity: 0.7 }}>
                    {t("未分配", "Unassigned")}
                  </span>
                ) : (
                  item.keys.map((key, idx) => {
                    const normalized = normalizeShortcutLabel(key);
                    const hasConflict = (conflictMap.get(normalized) ?? []).length > 1;
                    return (
                      <div
                        key={`${item.id}-${key}-${idx}`}
                        style={{
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "space-between",
                          background: hasConflict ? "rgba(224, 80, 80, 0.1)" : "var(--field)",
                          border: `1px solid ${hasConflict ? "rgba(224, 80, 80, 0.32)" : "var(--line-soft)"}`,
                          borderRadius: "7px",
                          paddingInline: "8px",
                          height: "24px",
                          fontSize: "11px",
                          color: hasConflict ? "oklch(67% 0.15 28)" : "var(--text)",
                          fontFamily: "var(--font-code)",
                          width: "100%",
                          maxWidth: "210px"
                        }}
                        title={hasConflict ? t("这个快捷键和其他命令冲突", "This shortcut conflicts with another command") : key}
                      >
                        <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{key}</span>
                        <button
                          onClick={() => handleDeleteBinding(item.id, idx)}
                          type="button"
                          aria-label={t("移除快捷键", "Remove shortcut")}
                          style={{
                            background: "transparent",
                            border: "none",
                            cursor: "pointer",
                            color: "var(--text-soft)",
                            padding: "1px 2px",
                            display: "flex",
                            alignItems: "center"
                          }}
                        >
                          <X size={12} />
                        </button>
                      </div>
                    );
                  })
                )}

                {recordingId === item.id ? (
                  <button
                    type="button"
                    autoFocus
                    onKeyDown={(event) => handleRecordKey(event, item.id)}
                    onBlur={() => setRecordingId(null)}
                    style={{
                      width: "100%",
                      maxWidth: "210px",
                      height: "26px",
                      border: "1px solid var(--accent)",
                      background: "color-mix(in oklab, var(--accent) 10%, var(--field))",
                      color: "var(--accent)",
                      borderRadius: "7px",
                      fontSize: "11px",
                      fontFamily: "var(--font-ui)"
                    }}
                  >
                    {t("按下新的快捷键...", "Press new shortcut...")}
                  </button>
                ) : (
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => setRecordingId(item.id)}
                    style={{ width: "100%", maxWidth: "210px", height: "26px", justifyContent: "center", gap: "6px" }}
                  >
                    <Plus size={12} />
                    {t("添加快捷键", "Add shortcut")}
                  </button>
                )}
              </div>
            </div>
          ))}
          {filteredBindings.length === 0 && (
            <div style={{ paddingBlock: "28px", textAlign: "center", color: "var(--text-soft)", fontSize: "12px" }}>
              {t("未找到匹配的快捷键命令", "No matching shortcut commands found")}
            </div>
          )}
        </div>
      </div>

      {statusText && (
        <div style={{ display: "flex", alignItems: "center", gap: "6px", fontSize: "11px", color: "var(--text-soft)" }}>
          <Check size={12} />
          {statusText}
        </div>
      )}
    </div>
  );
}
