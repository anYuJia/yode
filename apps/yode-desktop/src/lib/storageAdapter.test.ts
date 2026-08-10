import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  runStorageMigrations,
  storageReadJson,
  storageReadString,
  storageWriteJson,
  storageWriteString
} from "./storageAdapter";

describe("storage adapter", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", createMemoryStorage());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reads JSON with fallback on missing or corrupted values", () => {
    expect(storageReadJson("missing-key", [])).toEqual([]);
    localStorage.setItem("broken", "{not json");
    expect(storageReadJson("broken", { fallback: true })).toEqual({ fallback: true });
    localStorage.setItem("valid", JSON.stringify({ a: 1 }));
    expect(storageReadJson("valid", null)).toEqual({ a: 1 });
  });

  it("writes JSON and strings through the adapter", () => {
    storageWriteJson("json-key", { list: [1, 2] });
    storageWriteString("str-key", "hello");
    expect(localStorage.getItem("json-key")).toBe('{"list":[1,2]}');
    expect(localStorage.getItem("str-key")).toBe("hello");
    expect(storageReadString("str-key", "fallback")).toBe("hello");
    expect(storageReadString("missing", "fallback")).toBe("fallback");
  });

  it("migrates legacy values exactly once and records progress", () => {
    localStorage.setItem("legacy-null", "null");
    localStorage.setItem("legacy-empty", "");
    localStorage.setItem("legacy-keep", "keep-me");

    const first = runStorageMigrations([
      { key: "legacy-null", migrate: (raw) => (raw === "null" ? "standalone" : undefined) },
      { key: "legacy-empty", migrate: (raw) => (raw === "" ? "default" : undefined) },
      { key: "legacy-keep", migrate: () => undefined }
    ]);
    expect(first).toBe(3);
    expect(localStorage.getItem("legacy-null")).toBe("standalone");
    expect(localStorage.getItem("legacy-empty")).toBe("default");
    expect(localStorage.getItem("legacy-keep")).toBe("keep-me");

    // 第二次运行：已迁移的 key 不再重复处理
    const second = runStorageMigrations([
      { key: "legacy-null", migrate: (raw) => "overwritten" },
      { key: "legacy-empty", migrate: (raw) => "overwritten" },
      { key: "legacy-keep", migrate: (raw) => "overwritten" }
    ]);
    expect(second).toBe(0);
    expect(localStorage.getItem("legacy-null")).toBe("standalone");
  });

  it("keeps raw string values when migration returns undefined", () => {
    localStorage.setItem("untouched", "value");
    runStorageMigrations([{ key: "untouched", migrate: () => undefined }]);
    expect(localStorage.getItem("untouched")).toBe("value");
  });
});

function createMemoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => {
      values.delete(key);
    },
    setItem: (key, value) => {
      values.set(key, value);
    }
  };
}
