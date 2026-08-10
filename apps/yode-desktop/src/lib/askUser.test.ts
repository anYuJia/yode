import { describe, expect, it } from "vitest";

import { formatAskUserAnswerForDisplay, isUserQuery, parseUserQueryJson } from "./askUser";
import { submitAskUserAnswer } from "../components/chat-workspace/AskUserActions";

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
    expect(isUserQuery(["not", "an", "object"])).toBe(false);
    expect(isUserQuery({ questions: [{ header: "Bad", question: "Nope", options: [null] }] })).toBe(false);
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

  it("keeps an AskUser submission retryable after failure", async () => {
    let attempts = 0;
    const onResolve = async (answer: string) => {
      attempts += 1;
      expect(answer).toBe("再次确认");
      return attempts === 2;
    };

    expect(await submitAskUserAnswer(onResolve, "再次确认")).toBe(false);
    expect(await submitAskUserAnswer(onResolve, "再次确认")).toBe(true);
    expect(attempts).toBe(2);
  });
});
