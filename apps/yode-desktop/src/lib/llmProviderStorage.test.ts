import { afterEach, describe, expect, it, vi } from "vitest";

import {
  DEFAULT_LLM_CHANGE_EVENT,
  dispatchDefaultLlmChange,
  lastModelStorageKey,
  loadLastModelForProvider,
  loadStoredProviderValues,
  loadStoredProvidersRaw,
  LLM_PROVIDERS_STORAGE_KEY,
  LLM_PROVIDERS_CHANGE_EVENT,
  modelsForProvider,
  modelsForProviderFromStorage,
  parseStoredProviderValues,
  parseStoredProviders,
  providerDisplayName,
  providerDisplayNameFromStorage,
  providerOptionsFromStorage,
  providerOptionsFromStoredProviders,
  preferredModelForProvider,
  preferredModelFromStorage,
  saveLastModelForProvider,
  saveStoredProviders
} from "./llmProviderStorage";

const meta = [
  { id: "openai", name: "OpenAI", nameEn: "OpenAI", defaultModels: ["gpt-5.5", "gpt-5.4"] },
  { id: "local", name: "本地模型", nameEn: "Local", defaultModels: ["llama"] }
];

describe("llm provider storage helpers", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("parses provider cache arrays and ignores invalid entries", () => {
    const raw = JSON.stringify([
      { id: "openai", models: ["gpt-5.5", 42, "gpt-5.4"] },
      { id: "broken" },
      null
    ]);

    expect(parseStoredProviders(raw)).toEqual([
      { id: "openai", models: ["gpt-5.5", "gpt-5.4"] }
    ]);
  });

  it("parses legacy provider cache objects", () => {
    const raw = JSON.stringify({
      openai: { id: "openai", models: ["stored"] }
    });

    expect(modelsForProvider("openai", raw, meta)).toEqual(["stored"]);
  });

  it("parses full provider values without leaking JSON parsing into views", () => {
    const raw = JSON.stringify({
      openai: { name: "OpenAI Custom", apiKey: "sk-test", models: ["stored"] },
      broken: null
    });

    expect(parseStoredProviderValues(raw)).toEqual([
      { id: "openai", name: "OpenAI Custom", apiKey: "sk-test", models: ["stored"] },
      { id: "broken" }
    ]);
    expect(parseStoredProviderValues("not-json")).toEqual([]);
  });

  it("falls back to provider metadata models", () => {
    expect(modelsForProvider("local", null, meta)).toEqual(["llama"]);
  });

  it("prefers the last model only when it is still available", () => {
    const raw = JSON.stringify([{ id: "openai", models: ["new", "old"] }]);

    expect(preferredModelForProvider("openai", raw, meta, "old")).toBe("old");
    expect(preferredModelForProvider("openai", raw, meta, "missing")).toBe("new");
  });

  it("formats last model storage keys", () => {
    expect(lastModelStorageKey("openai")).toBe("yode-last-model-openai");
  });

  it("builds enabled provider options from storage", () => {
    const raw = JSON.stringify([
      { id: "openai", name: "Custom OpenAI", enabled: true, models: ["gpt"] },
      { id: "local", name: "Local", enabled: false, models: ["llama"] }
    ]);

    expect(providerOptionsFromStorage(raw, meta)).toEqual([
      { value: "openai", label: "Custom OpenAI" }
    ]);
  });

  it("loads provider display data from the shared storage helpers", () => {
    stubMemoryLocalStorage({
      [LLM_PROVIDERS_STORAGE_KEY]: JSON.stringify([
        { id: "openai", name: "Custom OpenAI", enabled: true, models: ["stored"] }
      ])
    });

    expect(providerOptionsFromStoredProviders(meta)).toEqual([
      { value: "openai", label: "Custom OpenAI" }
    ]);
    expect(providerDisplayNameFromStorage("openai", meta)).toBe("Custom OpenAI");
    expect(modelsForProviderFromStorage("openai", meta)).toEqual(["stored"]);
  });

  it("falls back to metadata provider names", () => {
    expect(providerDisplayName("local", null, meta)).toBe("本地模型");
    expect(providerDisplayName("missing", null, meta)).toBe("missing");
  });

  it("persists provider cache and last models", () => {
    const dispatchEvent = vi.fn();
    stubMemoryLocalStorage();
    vi.stubGlobal("window", { dispatchEvent });

    saveStoredProviders([{ id: "openai", models: ["stored"], enabled: true }]);
    saveLastModelForProvider("openai", "stored");

    expect(loadStoredProvidersRaw()).toBe(localStorage.getItem(LLM_PROVIDERS_STORAGE_KEY));
    expect(loadLastModelForProvider("openai")).toBe("stored");
    expect(preferredModelFromStorage("openai", meta)).toBe("stored");
    expect(loadStoredProviderValues()).toEqual([{ id: "openai", models: ["stored"], enabled: true }]);
    expect(dispatchEvent).toHaveBeenCalledWith(expect.objectContaining({ type: LLM_PROVIDERS_CHANGE_EVENT }));
  });

  it("dispatches default llm changes through one shared helper", () => {
    const dispatchEvent = vi.fn();
    vi.stubGlobal("window", { dispatchEvent });

    dispatchDefaultLlmChange({ provider: "openai", model: "stored" });

    expect(dispatchEvent).toHaveBeenCalledTimes(1);
    expect(dispatchEvent.mock.calls[0]?.[0]).toMatchObject({
      type: DEFAULT_LLM_CHANGE_EVENT,
      detail: { provider: "openai", model: "stored" }
    });
  });
});

function stubMemoryLocalStorage(seed: Record<string, string> = {}) {
  const values = new Map(Object.entries(seed));
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
    removeItem: (key: string) => {
      values.delete(key);
    }
  });
}
