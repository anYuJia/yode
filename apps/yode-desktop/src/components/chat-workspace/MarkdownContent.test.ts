import { describe, expect, it } from "vitest";
import { marked } from "marked";
import { preprocessMarkdown } from "./MarkdownContent";

describe("preprocessMarkdown", () => {
  function lex(source: string) {
    return marked.lexer(preprocessMarkdown(source));
  }

  it("closes a loose code fence before bold prose sections", () => {
    const source = [
      "```",
      "app/ ├── api/routes/ ← 路由层",
      "├── services/ ← 业务逻辑层",
      "",
      "**亮点**:",
      "",
      "- Repository 模式：数据访问层与业务逻辑分离",
      "- Alembic 迁移：数据库演进有记录",
    ].join("\n");

    const tokens = lex(source);
    const codeText = tokens
      .filter((token) => token.type === "code")
      .map((token: any) => token.text || "")
      .join("\n");
    const hasList = tokens.some((token) => token.type === "list");
    const nonCodeText = tokens
      .filter((token) => token.type !== "code")
      .map((token) => ("raw" in token ? token.raw : ""))
      .join("\n");

    expect(codeText).toContain("api/routes");
    expect(codeText).not.toContain("亮点");
    expect(nonCodeText).toContain("亮点");
    expect(hasList).toBe(true);
  });

  it("recovers loose fenced tree blocks before common prose blocks", () => {
    const cases = [
      ["裸中文小标题", "亮点：\n- 分层清晰"],
      ["ATX 标题", "## 后端分析\n正文"],
      ["无空格 ATX 标题", "##后端分析\n正文"],
      ["引用", "> 这里是结论"],
      ["表格", "| 模块 | 说明 |\n| --- | --- |\n| api | 路由 |"],
      ["分割线", "---\n后续正文"],
    ];

    for (const [name, tail] of cases) {
      const source = ["```", "app/ ├── api/routes/", "├── services/", "", tail].join("\n");
      const tokens = lex(source);
      const codeText = tokens
        .filter((token) => token.type === "code")
        .map((token: any) => token.text || "")
        .join("\n");
      const nonCodeText = tokens
        .filter((token) => token.type !== "code")
        .map((token) => ("raw" in token ? token.raw : ""))
        .join("\n");

      expect(codeText, name).toContain("api/routes");
      expect(nonCodeText.trim(), name).not.toBe("");
    }
  });

  it("does not close explicit language code blocks on markdown-looking code", () => {
    const source = [
      "```ts",
      "const markdown = '**亮点**:';",
      "const list = '- Repository 模式';",
      "```",
    ].join("\n");
    const tokens = lex(source);
    const codeTokens = tokens.filter((token) => token.type === "code") as any[];

    expect(codeTokens).toHaveLength(1);
    expect(codeTokens[0].text).toContain("**亮点**");
    expect(codeTokens[0].text).toContain("Repository");
  });
});
