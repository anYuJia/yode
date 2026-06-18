import { describe, expect, it } from "vitest";

import {
  lastModelStorageKey,
  modelsForProvider,
  parseStoredProviders,
  preferredModelForProvider
} from "./llmProviderStorage";

const meta = [
  { id: "openai", defaultModels: ["gpt-5.5", "gpt-5.4"] },
  { id: "local", defaultModels: ["llama"] }
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
});
