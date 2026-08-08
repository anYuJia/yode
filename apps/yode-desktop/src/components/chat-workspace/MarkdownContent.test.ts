import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { marked } from "marked";
import type { Token, Tokens } from "marked";
import { MarkdownContent, preprocessMarkdown, renderInlineMarkdown } from "./MarkdownContent";
import { hasCodeBlockContent } from "./codeBlockContent";

function isCodeToken(token: Token): token is Tokens.Code {
  return token.type === "code";
}

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

    const processed = preprocessMarkdown(source);
    const tokens = marked.lexer(processed);
    const codeText = tokens
      .filter(isCodeToken)
      .map((token) => token.text || "")
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
        .filter(isCodeToken)
        .map((token) => token.text || "")
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
    const processed = preprocessMarkdown(source);
    const tokens = marked.lexer(processed);
    const codeTokens = tokens.filter(isCodeToken);

    expect(codeTokens).toHaveLength(1);
    expect(codeTokens[0].text).toContain("**亮点**");
    expect(codeTokens[0].text).toContain("Repository");
  });

  it("wraps unfenced tree-like file listings as code instead of broken table text", () => {
    const source = [
      "新文件清单：",
      "src/scanners/system/ |—— __init__.py |—— port_scanner.py",
      "",
      "端口扫描",
      "|—— service_detector.py",
      "",
      "配置审计",
      "src/scanners/dependency/ |—— __init__.py |—— python_deps.py",
    ].join("\n");

    const processed = preprocessMarkdown(source);
    const tokens = marked.lexer(processed);
    const codeText = tokens
      .filter(isCodeToken)
      .map((token) => token.text || "")
      .join("\n");

    expect(processed).toContain("```text\nsrc/scanners/system/");
    expect(codeText).toContain("src/scanners/system/");
    expect(codeText).toContain("service_detector.py");
    expect(tokens.some((token) => token.type === "table")).toBe(false);
  });

  it("keeps valid markdown tables renderable as tables", () => {
    const tokens = lex([
      "| 模块 | 说明 |",
      "| --- | --- |",
      "| api | 路由 |",
    ].join("\n"));

    expect(tokens.some((token) => token.type === "table")).toBe(true);
  });

  it("treats empty fenced code blocks as non-renderable", () => {
    expect(hasCodeBlockContent("")).toBe(false);
    expect(hasCodeBlockContent("\n  \n")).toBe(false);
    expect(hasCodeBlockContent("\u200B\n")).toBe(false);
    expect(hasCodeBlockContent("const value = 1;")).toBe(true);
  });
});

describe("MarkdownContent URL policy", () => {
  function renderMarkdown(text: string) {
    return renderToStaticMarkup(createElement(MarkdownContent, { text }));
  }

  it("renders only approved external links with safe new-window attributes", () => {
    const markup = renderMarkdown("[网站](HTTPS://example.com/docs) [邮件](mailto:hello@example.com)");

    expect(markup).toContain('href="https://example.com/docs"');
    expect(markup).toContain('href="mailto:hello@example.com"');
    expect(markup).toContain('target="_blank"');
    expect(markup).toContain('rel="noopener noreferrer"');
  });

  it("renders safe page anchors in the current page", () => {
    const markup = renderMarkdown("[目录](#section-one)");

    expect(markup).toContain('href="#section-one"');
    expect(markup).not.toContain('target="_blank"');
  });

  it("renders unsafe links as their formatted visible content without anchors", () => {
    const markup = renderMarkdown([
      "[**脚本**](javascript:alert(1))",
      "[数据](data:text/html,unsafe)",
      "[文件](file:///etc/passwd)",
      "[应用](tauri://localhost/settings)",
      "[混合大小写](JaVaScRiPt:alert(1))",
      "[控制字符](\u007FJaVaScRiPt:alert(1))",
      "[相对路径](./settings)",
      "[协议相对](//example.com)",
    ].join(" "));

    expect(markup).toContain("<strong>脚本</strong>");
    expect(markup).toContain("数据");
    expect(markup).toContain("文件");
    expect(markup).toContain("应用");
    expect(markup).toContain("混合大小写");
    expect(markup).toContain("控制字符");
    expect(markup).toContain("相对路径");
    expect(markup).toContain("协议相对");
    expect(markup).not.toContain("<a");
    expect(markup).not.toContain("javascript:");
    expect(markup).not.toContain("data:text/html");
    expect(markup).not.toContain("file://");
    expect(markup).not.toContain("tauri://");
  });

  it("does not render unsafe markdown image sources", () => {
    const markup = renderMarkdown([
      "![数据图片](data:image/svg+xml,unsafe)",
      "![本地图片](file:///tmp/image.png)",
      "![应用图片](tauri://localhost/image.png)",
      "![远程图片](https://example.com/image.png)",
    ].join(" "));

    expect(markup).toContain("数据图片");
    expect(markup).toContain("本地图片");
    expect(markup).toContain("应用图片");
    expect(markup).toContain('<img src="https://example.com/image.png"');
    expect(markup).not.toContain('src="data:');
    expect(markup).not.toContain('src="file:');
    expect(markup).not.toContain('src="tauri:');
  });

  it("applies the same policy to inline markdown for mixed-case and control-character URLs", () => {
    const markup = renderToStaticMarkup(renderInlineMarkdown([
      "[脚本](JaVaScRiPt:alert(1))",
      "[数据](DaTa:text/html,unsafe)",
      "[文件](FiLe:///etc/passwd)",
      "[应用](TaUrI://localhost/settings)",
      "[控制字符](\u007FFiLe:///tmp/secret)",
    ].join(" ")));

    expect(markup).toContain("脚本");
    expect(markup).toContain("数据");
    expect(markup).toContain("文件");
    expect(markup).toContain("应用");
    expect(markup).toContain("控制字符");
    expect(markup).not.toContain("<a");
    expect(markup).not.toContain("href=");
  });
});
