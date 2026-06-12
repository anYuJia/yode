import React, { useMemo } from "react";
import { getFileIcon, fileIconMeta } from "../FileIcon";
import { CodeBlock } from "./CodeBlock";

type MarkdownVariant = "answer" | "process";

export function MarkdownContent({ text, variant = "answer" }: { text: string; variant?: MarkdownVariant }) {
  const blocks = useMemo(() => parseMarkdownBlocks(text), [text]);
  return (
    <div className={`markdown-content markdown-content-${variant}`}>
      {blocks.map((block, index) => {
        if (block.type === "heading") {
          const Tag = `h${Math.min(block.level, 4)}` as keyof JSX.IntrinsicElements;
          return <Tag key={index}>{renderInlineMarkdown(block.text)}</Tag>;
        }
        if (block.type === "code") {
          return <CodeBlock key={index} text={block.text} lang={block.lang} />;
        }
        if (block.type === "list") {
          const ListTag = block.ordered ? "ol" : "ul";
          return (
            <ListTag key={index} style={{ paddingLeft: "20px", listStyleType: block.ordered ? "decimal" : "disc" }}>
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
              ))}
            </ListTag>
          );
        }
        if (block.type === "table") {
          return (
            <div key={index} className="markdown-table-wrapper" style={{ overflowX: "auto", margin: "12px 0" }}>
              <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "12px" }}>
                <thead>
                  <tr style={{ borderBottom: "2px solid var(--line)" }}>
                    {block.headers.map((h, i) => (
                      <th key={i} style={{ padding: "8px", textAlign: "left", fontWeight: "bold" }}>
                        {renderInlineMarkdown(h)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {block.rows.map((row, ri) => (
                    <tr key={ri} style={{ borderBottom: "1px solid var(--line-soft)" }}>
                      {row.map((cell, ci) => (
                        <td key={ci} style={{ padding: "8px" }}>
                          {renderInlineMarkdown(cell)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        }
        if (block.type === "divider") {
          return <hr key={index} style={{ border: "0", borderTop: "1px solid var(--line-soft)", margin: "16px 0" }} />;
        }
        return <p key={index}>{renderInlineMarkdown(block.text)}</p>;
      })}
    </div>
  );
}

type MarkdownBlock =
  | { type: "heading"; level: number; text: string }
  | { type: "code"; text: string; lang: string }
  | { type: "list"; ordered: boolean; items: string[] }
  | { type: "table"; headers: string[]; rows: string[][] }
  | { type: "divider" }
  | { type: "paragraph"; text: string };

export function parseMarkdownBlocks(text: string): MarkdownBlock[] {
  const blocks: MarkdownBlock[] = [];
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  let paragraph: string[] = [];
  
  let currentListItems: string[] = [];
  let currentListOrdered = false;

  let tableRows: string[][] = [];
  let code: string[] | null = null;
  const flushParagraph = () => {
    if (paragraph.length > 0) {
      blocks.push({ type: "paragraph", text: paragraph.join(" ") });
      paragraph = [];
    }
  };
  const flushList = () => {
    if (currentListItems.length > 0) {
      blocks.push({ type: "list", ordered: currentListOrdered, items: currentListItems });
      currentListItems = [];
    }
  };
  const flushTable = () => {
    if (tableRows.length > 0) {
      if (tableRows.length >= 2 && tableRows[1].every(cell => /^:?-+:?$/.test(cell.trim()))) {
        const headers = tableRows[0];
        const rows = tableRows.slice(2);
        blocks.push({ type: "table", headers, rows });
      } else {
        for (const row of tableRows) {
          paragraph.push("|" + row.join("|") + "|");
        }
      }
      tableRows = [];
    }
  };
  let codeLang = "";
  for (const line of lines) {
    const trimmed = line.trim();
    const fenceMatch = trimmed.match(/^(?:```|｀｀｀)(.*)$/);
    const isClosingFence = code && /^(?:```|｀｀｀|、、|、、、|``)$/.test(trimmed);

    if (fenceMatch || isClosingFence) {
      if (code) {
        blocks.push({ type: "code", text: code.join("\n"), lang: codeLang });
        code = null;
        codeLang = "";
      } else if (fenceMatch) {
        flushParagraph();
        flushList();
        flushTable();
        codeLang = fenceMatch[1].trim().toLowerCase();
        code = [];
      }
      continue;
    }

    if (code) {
      code.push(line);
      continue;
    }

    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) {
      flushParagraph();
      flushList();
      flushTable();
      blocks.push({ type: "heading", level: heading[1].length, text: heading[2].trim() });
      continue;
    }

    const unorderedMatch = line.match(/^\s*[-*]\s+(.+)$/);
    const orderedMatch = line.match(/^\s*(\d+)[.)]\s+(.+)$/);

    if (unorderedMatch) {
      flushParagraph();
      flushTable();
      if (currentListItems.length > 0 && currentListOrdered) {
        flushList();
      }
      currentListOrdered = false;
      currentListItems.push(unorderedMatch[1].trim());
      continue;
    }

    if (orderedMatch) {
      flushParagraph();
      flushTable();
      if (currentListItems.length > 0 && !currentListOrdered) {
        flushList();
      }
      currentListOrdered = true;
      currentListItems.push(orderedMatch[2].trim());
      continue;
    }

    const isDivider = /^(?:-{3,}|\*{3,}|_{3,})$/.test(line.trim());
    if (isDivider) {
      flushParagraph();
      flushList();
      flushTable();
      blocks.push({ type: "divider" });
      continue;
    }

    const isTableRow = line.trim().startsWith("|") && line.trim().endsWith("|");
    if (isTableRow) {
      flushParagraph();
      flushList();
      const cells = line.split("|").map(c => c.trim()).slice(1, -1);
      tableRows.push(cells);
      continue;
    }

    if (!line.trim()) {
      flushParagraph();
      flushList();
      flushTable();
      continue;
    }

    flushList();
    flushTable();
    paragraph.push(line.trim());
  }

  if (code) blocks.push({ type: "code", text: code.join("\n"), lang: codeLang });
  flushParagraph();
  flushList();
  flushTable();
  return blocks.length > 0 ? blocks : [{ type: "paragraph", text }];
}

export function renderInlineMarkdown(text: string) {
  const parts = text.split(/(`[^`]+`|\*\*[^*]+\*\*)/g).filter(Boolean);
  return parts.map((part, index) => {
    if (part.startsWith("`") && part.endsWith("`")) {
      let codeText = part.slice(1, -1);
      
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
            key={index}
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
          <code key={index} style={{ 
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
          <code key={index} style={{ 
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
          <code key={index} style={{ 
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
      
      return <code key={index}>{codeText}</code>;
    }
    if (part.startsWith("**") && part.endsWith("**")) {
      return <strong key={index}>{part.slice(2, -2)}</strong>;
    }
    return <React.Fragment key={index}>{part}</React.Fragment>;
  });
}
