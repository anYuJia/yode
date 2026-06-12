import React, { useMemo } from "react";
import { getFileIcon, fileIconMeta } from "../FileIcon";
import { CodeBlock } from "./CodeBlock";
import { marked } from "marked";

type MarkdownVariant = "answer" | "process";

function preprocessMarkdown(text: string): string {
  const lines = text.split("\n");
  let inCodeBlock = false;
  const normalizedLines: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();
    if (trimmed.startsWith("```") || trimmed.startsWith("｀｀｀")) {
      inCodeBlock = !inCodeBlock;
      normalizedLines.push(line);
      continue;
    }
    if (inCodeBlock) {
      normalizedLines.push(line);
      continue;
    }

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
  const finalLines: string[] = [];
  for (let i = 0; i < normalizedLines.length; i++) {
    const line = normalizedLines[i];
    const trimmed = line.trim();
    if (trimmed.startsWith("```") || trimmed.startsWith("｀｀｀")) {
      inCodeBlock = !inCodeBlock;
      finalLines.push(line);
      continue;
    }
    if (inCodeBlock) {
      finalLines.push(line);
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

export function MarkdownContent({ text, variant = "answer" }: { text: string; variant?: MarkdownVariant }) {
  const processedText = useMemo(() => preprocessMarkdown(text), [text]);
  const tokens = useMemo(() => marked.lexer(processedText), [processedText]);

  return (
    <div className={`markdown-content markdown-content-${variant}`}>
      <RenderTokens tokens={tokens} />
    </div>
  );
}

export function renderInlineMarkdown(text: string) {
  const processedText = preprocessMarkdown(text);
  const tokens = marked.lexer(processedText);
  return <RenderTokens tokens={tokens} />;
}

function RenderTokens({ tokens }: { tokens: any[] }) {
  return (
    <>
      {tokens.map((token, index) => (
        <RenderToken key={index} token={token} />
      ))}
    </>
  );
}

function RenderToken({ token }: { token: any }): React.ReactElement | null {
  switch (token.type) {
    case "heading": {
      const Tag = `h${Math.min(token.depth, 4)}` as keyof JSX.IntrinsicElements;
      return (
        <Tag>
          <RenderTokens tokens={token.tokens} />
        </Tag>
      );
    }
    case "code": {
      return <CodeBlock text={token.text} lang={token.lang || ""} />;
    }
    case "list": {
      const ListTag = token.ordered ? "ol" : "ul";
      return (
        <ListTag style={{ paddingLeft: "20px", listStyleType: token.ordered ? "decimal" : "disc" }}>
          {token.items.map((item: any, idx: number) => (
            <li key={idx}>
              {item.task && (
                <input
                  type="checkbox"
                  checked={item.checked}
                  readOnly
                  style={{ marginRight: "6px", verticalAlign: "middle" }}
                />
              )}
              <RenderTokens tokens={item.tokens} />
            </li>
          ))}
        </ListTag>
      );
    }
    case "table": {
      return (
        <div className="markdown-table-wrapper" style={{ overflowX: "auto", margin: "12px 0" }}>
          <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "12px" }}>
            <thead>
              <tr style={{ borderBottom: "2px solid var(--line)" }}>
                {token.header.map((cell: any, i: number) => (
                  <th key={i} style={{ padding: "8px", textAlign: cell.align || "left", fontWeight: "bold" }}>
                    <RenderTokens tokens={cell.tokens} />
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {token.rows.map((row: any, ri: number) => (
                <tr key={ri} style={{ borderBottom: "1px solid var(--line-soft)" }}>
                  {row.map((cell: any, ci: number) => (
                    <td key={ci} style={{ padding: "8px", textAlign: cell.align || "left" }}>
                      <RenderTokens tokens={cell.tokens} />
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
      return (
        <p>
          <RenderTokens tokens={token.tokens} />
        </p>
      );
    }
    case "blockquote": {
      return (
        <blockquote style={{ borderLeft: "4px solid var(--line-soft)", paddingLeft: "12px", margin: "8px 0", color: "var(--text-muted)" }}>
          <RenderTokens tokens={token.tokens} />
        </blockquote>
      );
    }
    case "space": {
      return null;
    }
    case "strong": {
      return (
        <strong>
          <RenderTokens tokens={token.tokens} />
        </strong>
      );
    }
    case "em": {
      return (
        <em>
          <RenderTokens tokens={token.tokens} />
        </em>
      );
    }
    case "codespan": {
      return renderCodespan(token.text);
    }
    case "link": {
      return (
        <a href={token.href} target="_blank" rel="noopener noreferrer" style={{ color: "var(--accent)", textDecoration: "underline" }}>
          <RenderTokens tokens={token.tokens} />
        </a>
      );
    }
    case "image": {
      return <img src={token.href} alt={token.text} style={{ maxWidth: "100%", height: "auto" }} />;
    }
    case "text": {
      if (token.tokens && token.tokens.length > 0) {
        return <RenderTokens tokens={token.tokens} />;
      }
      return <>{token.text}</>;
    }
    case "br": {
      return <br />;
    }
    case "html": {
      return <>{token.text}</>;
    }
    default: {
      if ("tokens" in token && token.tokens) {
        return <RenderTokens tokens={token.tokens} />;
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
    return (
      <code style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "4px",
        verticalAlign: "middle",
        padding: "1px 6px",
        border: "1px solid color-mix(in oklch, var(--accent), transparent 75%)",
        background: "color-mix(in oklch, var(--accent), transparent 94%)",
        color: "var(--accent)"
      }}>
        <span style={{ fontSize: "9px", opacity: 0.6, fontWeight: 700, fontFamily: "system-ui" }}>cls</span>
        <strong>{codeText}</strong>
      </code>
    );
  }

  const isFunction = /^[a-zA-Z_][a-zA-Z0-9_]*\s*\([^)]*\)$/.test(codeText);
  if (isFunction) {
    return (
      <code style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "4px",
        verticalAlign: "middle",
        padding: "1px 6px",
        border: "1px solid color-mix(in oklch, var(--info), transparent 75%)",
        background: "color-mix(in oklch, var(--info), transparent 94%)",
        color: "var(--info)"
      }}>
        <span style={{ fontSize: "9px", opacity: 0.6, fontWeight: 700, fontFamily: "system-ui" }}>fn</span>
        <span>{codeText}</span>
      </code>
    );
  }

  const isVariable = /^[a-zA-Z_][a-zA-Z0-9_]*$/.test(codeText) && !/^[A-Z0-9_]+$/.test(codeText);
  if (isVariable) {
    return (
      <code style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "4px",
        verticalAlign: "middle",
        padding: "1px 6px",
        border: "1px solid color-mix(in oklch, var(--warning), transparent 75%)",
        background: "color-mix(in oklch, var(--warning), transparent 94%)",
        color: "var(--warning)"
      }}>
        <span style={{ fontSize: "9px", opacity: 0.6, fontWeight: 700, fontFamily: "system-ui" }}>var</span>
        <span>{codeText}</span>
      </code>
    );
  }

  return <code>{codeText}</code>;
}
