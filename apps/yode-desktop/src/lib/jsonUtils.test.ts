import { describe, expect, it } from "vitest";

import { parseJsonArray, parseJsonObject, recordFromUnknown } from "./jsonUtils";

describe("json utils", () => {
  it("narrows plain records without accepting arrays", () => {
    expect(recordFromUnknown({ ok: true })).toEqual({ ok: true });
    expect(recordFromUnknown(["nope"])).toBeUndefined();
    expect(recordFromUnknown(null)).toBeUndefined();
  });

  it("parses only JSON objects for object reads", () => {
    expect(parseJsonObject(JSON.stringify({ cmd: "git status" }))).toEqual({ cmd: "git status" });
    expect(parseJsonObject(JSON.stringify(["not", "object"]))).toBeNull();
    expect(parseJsonObject("{bad json")).toBeNull();
  });

  it("parses only JSON arrays for array reads", () => {
    expect(parseJsonArray(JSON.stringify([{ name: "exec_command" }]))).toEqual([{ name: "exec_command" }]);
    expect(parseJsonArray(JSON.stringify({ name: "exec_command" }))).toEqual([]);
    expect(parseJsonArray("{bad json")).toEqual([]);
  });
});
