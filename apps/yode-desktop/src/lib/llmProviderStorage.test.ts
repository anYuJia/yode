import { describe, expect, it } from "vitest";

import {
  lastModelStorageKey,
  modelsForProvider,
  parseStoredProviders,
  providerDisplayName,
  providerOptionsFromStorage,
  preferredModelForProvider
} from "./llmProviderStorage";

const meta = [
  { id: "openai", name: "OpenAI", nameEn: "OpenAI", defaultModels: ["gpt-5.5", "gpt-5.4"] },
  { id: "local", name: "本地模型", nameEn: "Local", defaultModels: ["llama"] }
];

describe("llm provider storage helpers", () => {
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

  it("falls back to metadata provider names", () => {
    expect(providerDisplayName("local", null, meta)).toBe("本地模型");
    expect(providerDisplayName("missing", null, meta)).toBe("missing");
  });
});
