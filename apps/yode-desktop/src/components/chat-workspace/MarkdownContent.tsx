import React, { useEffect, useMemo, useRef, useState } from "react";
import { getFileIcon, fileIconMeta } from "../FileIcon";
import { CodeBlock } from "./CodeBlock";
import { hasCodeBlockContent } from "./codeBlockContent";
import { marked } from "marked";
import type { Token, Tokens } from "marked";

type MarkdownVariant = "answer" | "process";

function stripHeadingMarker(text: string): string {
  return text.replace(/^\s{0,3}#{1,6}\s+/, "").replace(/\s+#{1,6}\s*$/, "");
}

function repairMarkdownBlockLine(line: string): string {
  return line
    .replace(/^\s{4,}(?=#{1,6}\s*[\p{L}\p{N}])/u, "")
    .replace(/^\s{4,}(?=(?:[-*+]|\d+\.)\s*[\p{L}\p{N}])/u, "")
    .replace(/([^\n#])(?=#{1,6}\s?[\p{L}\p{N}])/gu, (matched, _precedingCharacter, offset, source) => {
      const hashIndex = offset + matched.length;
      const beforeHash = source.slice(0, hashIndex);
      const linkDestinationStart = beforeHash.lastIndexOf("](");
      const lastClosingParenthesis = beforeHash.lastIndexOf(")");
      return linkDestinationStart > lastClosingParenthesis ? matched : `${matched}\n\n`;
    })
    .replace(/(^|\n)(#{1,6})(?=\S)/g, "$1$2 ")
    .replace(/(^|\n)([-+])(?=\S)/g, "$1$2 ")
    .replace(/(^|\n)\*(?=[^\s*])/g, "$1* ")
    .replace(/(^|\n)(\d+\.)(?=\S)/g, "$1$2 ");
}

function codeFenceInfo(line: string) {
  const trimmed = line.trim();
  const match = trimmed.match(/^(`{3,}|｀{3,})(.*)$/);
  if (!match) return null;
  return {
    marker: match[1],
    lang: match[2].trim().toLowerCase()
  };
}

function isBrokenFenceClose(line: string) {
  return /^(`{2}|｀{2})\s*$/.test(line.trim());
}

function isFenceClose(line: string, openMarker: string) {
  const trimmed = line.trim();
  if (!openMarker) return false;
  const markerChar = openMarker[0];
  if (markerChar !== "`" && markerChar !== "｀") return false;
  const match = trimmed.match(/^(`{3,}|｀{3,})\s*$/);
  return Boolean(match && match[1][0] === markerChar && match[1].length >= openMarker.length);
}

function looksLikeMarkdownAfterTextFence(line: string) {
  const trimmed = line.trim();
  if (!trimmed) return false;
  if (/[├└│┬┴┼─━╭╰╮╯←→]/.test(trimmed)) return false;
  return /^#{1,6}\s+\S/.test(trimmed) ||
    /^#{1,6}(?=[\p{L}\p{N}])/u.test(trimmed) ||
    /^(?:[-*+]|\d+\.)\s+\S/.test(trimmed) ||
    /^>\s+\S/.test(trimmed) ||
    /^(?:\*\*|__)[^*_].*(?:\*\*|__)\s*[:：]?.*$/.test(trimmed) ||
    /^(?:---+|\*\*\*+|___+)\s*$/.test(trimmed) ||
    /^\|.+\|\s*$/.test(trimmed) ||
    /^[\p{L}\p{N}][\p{L}\p{N}\s·・、/()（）-]{1,36}[:：]\s*$/u;
}

function isLooseTextFence(lang: string) {
  return !lang || /^(text|txt|plain|plaintext|markdown|md)$/i.test(lang);
}

function looksLikeTreeLine(line: string) {
  const trimmed = line.trim();
  if (!trimmed) return false;
  if (/^[│├└┌┐┘┴┬┼─━╭╰╮╯]/.test(trimmed)) return true;
  if (trimmed.includes("/") && (trimmed.includes("|——") || trimmed.includes("｜——") || trimmed.includes("│——"))) return true;
  if (/[│├└]\s*[─━-]{2,}/.test(trimmed)) return true;
  if (trimmed.includes("/") && /[│|｜]\s*[─━—-]{2,}/.test(trimmed)) return true;
  if (/^[^│|｜]+\/\s*[│|｜]\s*[─━—-]{2,}\s*\S+/.test(trimmed)) return true;
  if (/^[│|｜]\s*[─━—-]{2,}\s*\S+\.[\w]+/.test(trimmed)) return true;
  return false;
}

function shouldStartTreeBlock(lines: string[], index: number) {
  if (!looksLikeTreeLine(lines[index])) return false;
  const previous = lines[index - 1]?.trim() || "";
  const next = lines[index + 1]?.trim() || "";
  if (looksLikeTreeLine(next)) return true;
  if (/^[\p{L}\p{N}][\p{L}\p{N}\s·・、/()（）.:-]{1,40}$/u.test(previous)) return true;
  return false;
}

function maybeCloseLooseFenceBeforeLine(
  line: string,
  inCodeBlock: boolean,
  codeBlockLang: string,
  codeBlockMarker: string,
  output: string[]
) {
  if (inCodeBlock && (isBrokenFenceClose(line) || isFenceClose(line, codeBlockMarker))) {
    output.push("```");
    return { handled: true, inCodeBlock: false, codeBlockLang: "", codeBlockMarker: "" };
  }

  if (inCodeBlock && isLooseTextFence(codeBlockLang) && looksLikeMarkdownAfterTextFence(line)) {
    output.push("```");
    return { handled: false, inCodeBlock: false, codeBlockLang: "", codeBlockMarker: "" };
  }

  return { handled: false, inCodeBlock, codeBlockLang, codeBlockMarker };
}

export function preprocessMarkdown(text: string): string {
  const lines = text.split("\n");
  let inCodeBlock = false;
  let codeBlockLang = "";
  let codeBlockMarker = "";
  const normalizedLines: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    let line = lines[i];
    const trimmed = line.trim();
    if (inCodeBlock && isBrokenFenceClose(line)) {
      inCodeBlock = false;
      codeBlockLang = "";
      codeBlockMarker = "";
      normalizedLines.push("```");
      continue;
    }

    const fence = codeFenceInfo(line);
    if (fence) {
      inCodeBlock = !inCodeBlock;
      codeBlockLang = inCodeBlock ? fence.lang : "";
      codeBlockMarker = inCodeBlock ? fence.marker : "";
      normalizedLines.push(line);
      continue;
    }
    if (inCodeBlock) {
      normalizedLines.push(line);
      continue;
    }

    line = repairMarkdownBlockLine(line);
    if (trimmed.startsWith("|")) {
      // Replace double pipes (which represent collapsed rows)
      let temp = line.replace(/\|\|/g, "|\n|");

      // If it still has a lot of pipes and looks like a collapsed table row with spaces (e.g. | | )
      if (temp.split("|").length > 8 && /\|\s+\|/.test(temp)) {
        temp = temp.replace(/\|\s+\|/g, "|\n|");
      }
      normalizedLines.push(...temp.split("\n"));
    } else {
      normalizedLines.push(line);
    }
  }

  inCodeBlock = false;
  codeBlockLang = "";
  codeBlockMarker = "";
  const finalLines: string[] = [];
  for (let i = 0; i < normalizedLines.length; i++) {
    const line = normalizedLines[i];
    const trimmed = line.trim();
    const looseFence = maybeCloseLooseFenceBeforeLine(line, inCodeBlock, codeBlockLang, codeBlockMarker, finalLines);
    inCodeBlock = looseFence.inCodeBlock;
    codeBlockLang = looseFence.codeBlockLang;
    codeBlockMarker = looseFence.codeBlockMarker;
    if (looseFence.handled) continue;

    const fence = codeFenceInfo(line);
    if (fence) {
      inCodeBlock = !inCodeBlock;
      codeBlockLang = inCodeBlock ? fence.lang : "";
      codeBlockMarker = inCodeBlock ? fence.marker : "";
      finalLines.push(line);
      continue;
    }
    if (inCodeBlock) {
      finalLines.push(line);
      continue;
    }

    if (
      shouldStartTreeBlock(normalizedLines, i) ||
      (trimmed.includes("/") && (trimmed.includes("|——") || trimmed.includes("｜——") || trimmed.includes("│——")))
    ) {
      if (finalLines.length > 0 && finalLines[finalLines.length - 1].trim()) {
        finalLines.push("");
      }
      finalLines.push("```text");
      while (i < normalizedLines.length) {
        const treeLine = normalizedLines[i];
        const treeTrimmed = treeLine.trim();
        if (!treeTrimmed) {
          finalLines.push(treeLine);
          i += 1;
          break;
        }
        if (!looksLikeTreeLine(treeLine) && !/^\s{2,}\S/.test(treeLine)) {
          i -= 1;
          break;
        }
        finalLines.push(treeLine);
        i += 1;
      }
      finalLines.push("```");
      if (normalizedLines[i + 1]?.trim()) {
        finalLines.push("");
      }
      continue;
    }

    // Check if the current line is a table delimiter
    const isDelimiter = /^[\s|:-]+$/.test(trimmed) && trimmed.includes("-") && trimmed.includes("|");
    if (isDelimiter && i > 0) {
      const headerLine = normalizedLines[i - 1];
      const headerTrimmed = headerLine.trim();
      if (headerTrimmed.includes("|")) {
        const countCols = (l: string) => {
          let cells = l.trim().split("|");
          if (cells[0] === "") cells.shift();
          if (cells[cells.length - 1] === "") cells.pop();
          return cells.length;
        };
        const headerCols = countCols(headerLine);
        const delimCols = countCols(line);
        if (headerCols > 0 && headerCols !== delimCols) {
          // Reconstruct the delimiter line to match the column count of the header
          const newDelimiter = "|" + Array(headerCols).fill("---").join("|") + "|";
          finalLines.push(newDelimiter);
          continue;
        }
      }
    }
    finalLines.push(line);
  }

  return finalLines.join("\n");
}

// marked 词法分析结果缓存：流式渲染期间文本逐帧增长，
// 相邻帧存在大量公共前缀，缓存最近解析结果避免每个 delta 全量 lexer。
const LEXER_CACHE_MAX = 48;
const lexerCache = new Map<string, Token[]>();

type MarkdownUrlKind = "link" | "image";

const DISALLOWED_URL_CHARACTERS = /[\u0000-\u001F\u007F-\u009F\s]/u;
const HTTP_URL_PATTERN = /^https?:\/\//i;
const MAILTO_URL_PATTERN = /^mailto:/i;

/**
 * Markdown is model-provided content, so only preserve URL forms that have an
 * explicit, safe navigation meaning. In particular, relative and protocol-
 * relative URLs must not inherit an application or custom-protocol origin.
 */
export function safeMarkdownUrl(rawUrl: string, kind: MarkdownUrlKind): string | null {
  if (!rawUrl || rawUrl.trim() !== rawUrl || DISALLOWED_URL_CHARACTERS.test(rawUrl)) {
    return null;
  }

  if (kind === "link" && rawUrl.startsWith("#")) {
    return rawUrl;
  }

  if (rawUrl.startsWith("//")) {
    return null;
  }

  let parsedUrl: URL;
  try {
    parsedUrl = new URL(rawUrl);
  } catch {
    return null;
  }

  if ((parsedUrl.protocol === "http:" || parsedUrl.protocol === "https:") && HTTP_URL_PATTERN.test(rawUrl)) {
    return parsedUrl.href;
  }

  if (kind === "link" && parsedUrl.protocol === "mailto:" && MAILTO_URL_PATTERN.test(rawUrl) && rawUrl.length > "mailto:".length) {
    return parsedUrl.href;
  }

  return null;
}

function lexerCached(processedText: string, cache = true): Token[] {
  if (!cache) return marked.lexer(processedText);
  const cached = lexerCache.get(processedText);
  if (cached) return cached;
  const tokens = marked.lexer(processedText);
  if (lexerCache.size >= LEXER_CACHE_MAX) {
    const oldest = lexerCache.keys().next().value;
    if (oldest !== undefined) lexerCache.delete(oldest);
  }
  lexerCache.set(processedText, tokens);
  return tokens;
}

export const MarkdownContent = React.memo(function MarkdownContent({
  text,
  variant = "answer",
  appLang,
  streaming = false
}: {
  text: string;
  variant?: MarkdownVariant;
  appLang?: string;
  streaming?: boolean;
}) {
  const [renderedText, setRenderedText] = useState(text);
  const latestTextRef = useRef(text);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    latestTextRef.current = text;
    // 短回答仍逐帧呈现；超过此阈值后最多每 120ms 做一次完整 Markdown 解析。
    if (!streaming || text.length <= 16 * 1024) {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      setRenderedText((current) => current === text ? current : text);
      return;
    }
    if (timerRef.current !== null) return;
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      setRenderedText((current) => current === latestTextRef.current ? current : latestTextRef.current);
    }, 120);
  }, [text, streaming]);

  useEffect(() => () => {
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
  }, []);

  const processedText = useMemo(() => preprocessMarkdown(renderedText), [renderedText]);
  const tokens = useMemo(
    () => lexerCached(processedText, !streaming || renderedText === text),
    [processedText, renderedText, streaming, text]
  );

  return (
    <div className={`markdown-content markdown-content-${variant}`}>
      <RenderTokens tokens={tokens} appLang={appLang} streaming={streaming && renderedText !== text} />
    </div>
  );
});

export function renderInlineMarkdown(text: string, appLang?: string) {
  const processedText = preprocessMarkdown(text);
  const tokens = lexerCached(processedText);
  return <RenderTokens tokens={tokens} appLang={appLang} />;
}

const RenderTokens = React.memo(function RenderTokens({
  tokens,
  appLang,
  streaming = false
}: {
  tokens: Token[];
  appLang?: string;
  streaming?: boolean;
}) {
  return (
    <>
      {tokens.map((token, index) => (
        <RenderToken key={index} token={token} appLang={appLang} streaming={streaming} />
      ))}
    </>
  );
});

function childTokens(tokens?: Token[]) {
  return tokens ?? [];
}

function RenderToken({ token, appLang, streaming = false }: {
  token: Token;
  appLang?: string;
  streaming?: boolean;
}): React.ReactElement | null {
  switch (token.type) {
    case "heading": {
      const heading = token as Tokens.Heading;
      const Tag = `h${Math.min(heading.depth, 4)}` as keyof JSX.IntrinsicElements;
      const text = stripHeadingMarker(heading.text);
      return <Tag>{text || <RenderTokens tokens={childTokens(heading.tokens)} appLang={appLang} streaming={streaming} />}</Tag>;
    }
    case "code": {
      const code = token as Tokens.Code;
      if (!hasCodeBlockContent(code.text)) return null;
      return <CodeBlock text={code.text} lang={code.lang || ""} appLang={appLang} streaming={streaming} />;
    }
    case "list": {
      const list = token as Tokens.List;
      const ListTag = list.ordered ? "ol" : "ul";
      return (
        <ListTag style={{ paddingLeft: "20px", listStyleType: list.ordered ? "decimal" : "disc" }}>
          {list.items.map((item, idx) => (
            <li key={idx}>
              {item.task && (
                <input
                  type="checkbox"
                  checked={item.checked}
                  readOnly
                  style={{ marginRight: "6px", verticalAlign: "middle" }}
                />
              )}
              <RenderTokens tokens={childTokens(item.tokens)} appLang={appLang} streaming={streaming} />
            </li>
          ))}
        </ListTag>
      );
    }
    case "table": {
      const table = token as Tokens.Table;
      return (
        <div className="markdown-table-wrapper" style={{ overflowX: "auto", margin: "12px 0" }}>
          <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "12px" }}>
            <thead>
              <tr style={{ borderBottom: "2px solid var(--line)" }}>
                {table.header.map((cell, i) => (
                  <th key={i} style={{ padding: "8px", textAlign: cell.align || "left", fontWeight: "bold" }}>
                    <RenderTokens tokens={childTokens(cell.tokens)} appLang={appLang} streaming={streaming} />
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {table.rows.map((row, ri) => (
                <tr key={ri} style={{ borderBottom: "1px solid var(--line-soft)" }}>
                  {row.map((cell, ci) => (
                    <td key={ci} style={{ padding: "8px", textAlign: cell.align || "left" }}>
                      <RenderTokens tokens={childTokens(cell.tokens)} appLang={appLang} streaming={streaming} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    }
    case "hr": {
      return <hr style={{ border: "0", borderTop: "1px solid var(--line-soft)", margin: "16px 0" }} />;
    }
    case "paragraph": {
      const paragraph = token as Tokens.Paragraph;
      const headingMatch = paragraph.text.match(/^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$/);
      if (headingMatch) {
        const Tag = `h${Math.min(headingMatch[1].length, 4)}` as keyof JSX.IntrinsicElements;
        return <Tag>{stripHeadingMarker(headingMatch[2])}</Tag>;
      }
      return (
        <p>
          <RenderTokens tokens={childTokens(paragraph.tokens)} appLang={appLang} streaming={streaming} />
        </p>
      );
    }
    case "blockquote": {
      const blockquote = token as Tokens.Blockquote;
      return (
        <blockquote style={{ borderLeft: "4px solid var(--line-soft)", paddingLeft: "12px", margin: "8px 0", color: "var(--text-muted)" }}>
          <RenderTokens tokens={childTokens(blockquote.tokens)} appLang={appLang} streaming={streaming} />
        </blockquote>
      );
    }
    case "space": {
      return null;
    }
    case "strong": {
      const strong = token as Tokens.Strong;
      return (
        <strong>
          <RenderTokens tokens={childTokens(strong.tokens)} appLang={appLang} streaming={streaming} />
        </strong>
      );
    }
    case "em": {
      const em = token as Tokens.Em;
      return (
        <em>
          <RenderTokens tokens={childTokens(em.tokens)} appLang={appLang} streaming={streaming} />
        </em>
      );
    }
    case "codespan": {
      const codespan = token as Tokens.Codespan;
      return renderCodespan(codespan.text);
    }
    case "link": {
      const link = token as Tokens.Link;
      const href = safeMarkdownUrl(link.href, "link");
      if (!href) {
        return <RenderTokens tokens={childTokens(link.tokens)} appLang={appLang} streaming={streaming} />;
      }

      if (href.startsWith("#")) {
        return <a href={href}><RenderTokens tokens={childTokens(link.tokens)} appLang={appLang} streaming={streaming} /></a>;
      }

      return (
        <a href={href} target="_blank" rel="noopener noreferrer" style={{ color: "var(--accent)", textDecoration: "underline" }}>
          <RenderTokens tokens={childTokens(link.tokens)} appLang={appLang} streaming={streaming} />
        </a>
      );
    }
    case "image": {
      const image = token as Tokens.Image;
      const src = safeMarkdownUrl(image.href, "image");
      if (!src) {
        return <>{image.text}</>;
      }
      return <img src={src} alt={image.text} style={{ maxWidth: "100%", height: "auto" }} />;
    }
    case "text": {
      const text = token as Tokens.Text;
      if (text.tokens && text.tokens.length > 0) {
        return <RenderTokens tokens={text.tokens} appLang={appLang} streaming={streaming} />;
      }
      return <>{text.text}</>;
    }
    case "br": {
      return <br />;
    }
    case "html": {
      const html = token as Tokens.HTML;
      return <>{html.text}</>;
    }
    default: {
      if ("tokens" in token && token.tokens) {
        return <RenderTokens tokens={token.tokens} appLang={appLang} streaming={streaming} />;
      }
      return <>{("text" in token ? (token.text as string) : "")}</>;
    }
  }
}

function renderCodespan(rawText: string) {
  let codeText = rawText;

  if ((codeText.startsWith("'") && codeText.endsWith("'")) || (codeText.startsWith('"') && codeText.endsWith('"'))) {
    codeText = codeText.slice(1, -1);
  }

  const isFilename = /^[a-zA-Z0-9_\-./\\]+\.[a-zA-Z0-9]+$/.test(codeText) || codeText.startsWith(".") || codeText.includes("/.") || codeText.includes("\\.");

  if (isFilename) {
    const parts = codeText.split(/[/\\]/);
    const baseName = parts[parts.length - 1] || codeText;
    const meta = fileIconMeta(baseName);

    return (
      <code
        className="markdown-code-chip markdown-file-chip"
        style={{
          "--chip-accent": meta.color
        } as React.CSSProperties}
      >
        <span className="markdown-file-chip-icon">
          {getFileIcon(baseName)}
        </span>
        <span className="markdown-file-chip-text">{codeText}</span>
      </code>
    );
  }

  const isClassName = /^[A-Z][a-zA-Z0-9]+$/.test(codeText);
  if (isClassName) {
    return <code className="markdown-code-chip">{codeText}</code>;
  }

  const isFunction = /^[a-zA-Z_][a-zA-Z0-9_]*\s*\([^)]*\)$/.test(codeText);
  if (isFunction) {
    return <code className="markdown-code-chip">{codeText}</code>;
  }

  const isVariable = /^[a-zA-Z_][a-zA-Z0-9_]*$/.test(codeText) && !/^[A-Z0-9_]+$/.test(codeText);
  if (isVariable) {
    return <code className="markdown-code-chip">{codeText}</code>;
  }

  return <code>{codeText}</code>;
}
