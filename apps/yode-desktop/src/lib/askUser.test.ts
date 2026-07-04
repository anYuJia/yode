import { describe, expect, it } from "vitest";

import { formatAskUserAnswerForDisplay, isUserQuery, parseUserQueryJson } from "./askUser";

describe("ask user helpers", () => {
  it("validates structured user queries", () => {
    const query = {
      questions: [
        {
          header: "Mode",
          question: "Choose mode",
          options: [{ label: "Fast", description: "Prioritize speed" }],
          multiSelect: false
        }
      ]
    };

    expect(isUserQuery(query)).toBe(true);
    expect(isUserQuery({ questions: [{ header: "Missing options", question: "Nope" }] })).toBe(false);
  });

  it("parses structured user query JSON safely", () => {
    expect(parseUserQueryJson("{not json")).toBeNull();
    expect(parseUserQueryJson(JSON.stringify({ questions: [] }))).toEqual({ questions: [] });
    expect(parseUserQueryJson(JSON.stringify({ questions: [{ header: "Bad" }] }))).toBeNull();
  });

  it("formats structured answers for timeline display", () => {
    expect(formatAskUserAnswerForDisplay(JSON.stringify({ Mode: "Fast", Scope: ["Core", "Desktop"] }))).toBe(
      "Fast, Core, Desktop"
    );
    expect(formatAskUserAnswerForDisplay("plain answer")).toBe("plain answer");
    expect(formatAskUserAnswerForDisplay(JSON.stringify(["not", "object"]))).toBe(JSON.stringify(["not", "object"]));
  });
});
