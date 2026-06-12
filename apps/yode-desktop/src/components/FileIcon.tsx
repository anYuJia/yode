import React from "react";
import { TerminalSquare } from "lucide-react";

export type FileIconMeta = {
  label: string;
  color: string;
  tone?: "code" | "config" | "doc" | "asset" | "data" | "plain";
  glyph?: "lock" | "image";
};

export function fileIconMeta(filename: string): FileIconMeta {
  const clean = filename.trim();
  const lower = clean.toLowerCase();
  const ext = clean.includes(".") ? clean.split(".").pop()?.toLowerCase() : "";

  if (lower === "package.json") return { label: "PKG", color: "#10B981", tone: "config" };
  if (lower === "cargo.toml" || lower === "cargo.lock") return { label: "toml", color: "#F97316", tone: "config" };
  if (lower === "pnpm-lock.yaml" || lower === "yarn.lock" || lower === "package-lock.json") return { label: "LOCK", color: "#94A3B8", tone: "config", glyph: "lock" };
  if (lower === "dockerfile" || lower.endsWith(".dockerfile")) return { label: "DO", color: "#38BDF8", tone: "config" };
  if (lower === "makefile") return { label: "MK", color: "#64748B", tone: "config" };
  if (lower === ".gitignore" || lower === ".gitattributes") return { label: "GIT", color: "#F43F5E", tone: "config" };
  if (lower === ".env" || lower.startsWith(".env.")) return { label: "env", color: "#FBBF24", tone: "config" };
  if (lower === "readme.md") return { label: "md", color: "#94A3B8", tone: "doc" };
  if (lower === "license") return { label: "TXT", color: "#64748B", tone: "doc" };

  switch (ext) {
    case "tsx": return { label: "tsx", color: "#60A5FA", tone: "code" };
    case "jsx": return { label: "jsx", color: "#FBBF24", tone: "code" };
    case "ts": return { label: "ts", color: "#3B82F6", tone: "code" };
    case "js":
    case "mjs":
    case "cjs": return { label: "js", color: "#FBBF24", tone: "code" };
    case "rs": return { label: "rs", color: "#F97316", tone: "code" };
    case "py": return { label: "py", color: "#38BDF8", tone: "code" };
    case "go": return { label: "go", color: "#22D3EE", tone: "code" };
    case "java": return { label: "JV", color: "#F97316", tone: "code" };
    case "c": return { label: "C", color: "#3B82F6", tone: "code" };
    case "cc":
    case "cpp":
    case "cxx":
    case "hpp": return { label: "C++", color: "#60A5FA", tone: "code" };
    case "swift": return { label: "SW", color: "#F97316", tone: "code" };
    case "kt":
    case "kts": return { label: "KT", color: "#A855F7", tone: "code" };
    case "rb": return { label: "RB", color: "#EF4444", tone: "code" };
    case "php": return { label: "PHP", color: "#8B5CF6", tone: "code" };
    case "css": return { label: "css", color: "#38BDF8", tone: "asset" };
    case "scss":
    case "sass": return { label: "SAS", color: "#EC4899", tone: "asset" };
    case "html":
    case "htm": return { label: "html", color: "#F97316", tone: "asset" };
    case "json": return { label: "json", color: "#34D399", tone: "data" };
    case "toml": return { label: "toml", color: "#A7F3D0", tone: "config" };
    case "yaml":
    case "yml": return { label: "yaml", color: "#059669", tone: "config" };
    case "xml": return { label: "XML", color: "#34D399", tone: "data" };
    case "sql": return { label: "SQL", color: "#3B82F6", tone: "data" };
    case "sqlite":
    case "db": return { label: "DB", color: "#60A5FA", tone: "data" };
    case "md":
    case "mdx": return { label: "md", color: "#94A3B8", tone: "doc" };
    case "txt":
    case "log": return { label: "txt", color: "#64748B", tone: "plain" };
    case "png":
    case "jpg":
    case "jpeg":
    case "gif":
    case "webp": return { label: "IMG", color: "#C084FC", tone: "asset", glyph: "image" };
    case "svg": return { label: "svg", color: "#F43F5E", tone: "asset" };
    case "pdf": return { label: "pdf", color: "#F87171", tone: "doc" };
    case "sh":
    case "bash":
    case "zsh": return { label: "sh", color: "#34D399", tone: "code" };
    default: return { label: ext ? ext.slice(0, 3).toLowerCase() : "file", color: "#64748B", tone: "plain" };
  }
}

export function getFileIcon(filename: string) {
  const meta = fileIconMeta(filename);
  const clean = filename.trim();
  const lower = clean.toLowerCase();
  const ext = clean.includes(".") ? clean.split(".").pop()?.toLowerCase() : "";

  const renderSvgBody = () => {
    if (lower === "package.json") {
      return (
        <>
          <path d="M5 2C5 2 3.5 2 3.5 3.5V6.5C3.5 7.5 2.5 8 2.5 8C2.5 8 3.5 8.5 3.5 9.5V12.5C3.5 14 5 14 5 14" />
          <path d="M11 2C11 2 12.5 2 12.5 3.5V6.5C12.5 7.5 13.5 8 13.5 8C13.5 8 12.5 8.5 12.5 9.5V12.5C12.5 14 11 14 11 14" />
        </>
      );
    }
    if (lower === "cargo.toml" || lower === "cargo.lock" || ext === "rs") {
      return (
        <>
          <circle cx="8" cy="8" r="4.5" strokeDasharray="2 1.5"/>
          <circle cx="8" cy="8" r="3"/>
          <path d="M8 2V1M8 15V14M2 8H1M15 8H14M3.7 3.7L3 3M13 13l-.7-.7M3.7 12.3L3 13M13 3l-.7.7" strokeLinecap="round"/>
          <text x="8" y="10" fill="currentColor" fontFamily="system-ui, sans-serif" fontSize="5.5" fontWeight="900" textAnchor="middle">R</text>
        </>
      );
    }
    if (meta.glyph === "lock") {
      return (
        <>
          <rect x="3" y="7.5" width="10" height="7" rx="1.5" />
          <path d="M5 7.5V4.5C5 2.84315 6.34315 1.5 8 1.5C9.65685 1.5 11 2.84315 11 4.5V7.5" />
          <circle cx="8" cy="10.5" r="1" fill="currentColor" />
        </>
      );
    }
    if (lower === "dockerfile" || lower.endsWith(".dockerfile")) {
      return (
        <>
          <rect x="3" y="2" width="2" height="1.8" rx="0.3" />
          <rect x="5.5" y="2" width="2" height="1.8" rx="0.3" />
          <rect x="8" y="2" width="2" height="1.8" rx="0.3" />
          <rect x="1.75" y="4.3" width="2" height="1.8" rx="0.3" />
          <rect x="4.25" y="4.3" width="2" height="1.8" rx="0.3" />
          <rect x="6.75" y="4.3" width="2" height="1.8" rx="0.3" />
          <rect x="9.25" y="4.3" width="2" height="1.8" rx="0.3" />
          <rect x="3" y="6.6" width="2" height="1.8" rx="0.3" />
          <rect x="5.5" y="6.6" width="2" height="1.8" rx="0.3" />
          <rect x="8" y="6.6" width="2" height="1.8" rx="0.3" />
          <path d="M1 9.5C3.5 10 5.5 10.5 8 10.5C10.5 10.5 12 9.8 13.5 8.5C14 8 14.5 8 14.8 8.5C15 9 14.8 10.2 14 11.2C12.5 13 9.5 13.5 7.5 13.5C4 13.5 2 12.2 1 9.5Z" fill="currentColor" fillOpacity="0.1"/>
        </>
      );
    }
    if (lower === ".gitignore" || lower === ".gitattributes" || ext === "git") {
      return (
        <>
          <rect x="3" y="3" width="10" height="10" rx="2.2" transform="rotate(45 8 8)" />
          <path d="M6 10V6M6 6C6 5 8 4.5 9.5 5.5" />
          <circle cx="6" cy="10" r="1.1" fill="currentColor" />
          <circle cx="6" cy="5.5" r="1.1" fill="currentColor" />
          <circle cx="10" cy="6" r="1.1" fill="currentColor" />
        </>
      );
    }

    switch (ext) {
      case "tsx":
        return (
          <>
            <ellipse cx="8" cy="8" rx="7" ry="2.5" transform="rotate(30 8 8)" />
            <ellipse cx="8" cy="8" rx="7" ry="2.5" transform="rotate(90 8 8)" />
            <ellipse cx="8" cy="8" rx="7" ry="2.5" transform="rotate(150 8 8)" />
            <circle cx="8" cy="8" r="1" fill="currentColor"/>
          </>
        );
      case "ts":
        return (
          <>
            <rect x="1" y="1" width="14" height="14" rx="3" fill="currentColor" fillOpacity="0.1" strokeWidth="1.2"/>
            <text x="8" y="11" fill="currentColor" fontFamily="system-ui, -apple-system, sans-serif" fontSize="7.5" fontWeight="800" textAnchor="middle" letterSpacing="-0.2">TS</text>
          </>
        );
      case "css":
        return (
          <>
            <line x1="5.5" y1="1.5" x2="3.5" y2="14.5" />
            <line x1="12.5" y1="1.5" x2="10.5" y2="14.5" />
            <line x1="1.5" y1="5.5" x2="14.5" y2="5.5" />
            <line x1="1.5" y1="10.5" x2="14.5" y2="10.5" />
          </>
        );
      case "py":
        return (
          <>
            <path d="M8 1.5H5.5C4.1 1.5 3 2.6 3 4V6.5C3 7.9 4.1 9 5.5 9H10.5C11.9 9 13 10.1 13 11.5V14C13 15.4 11.9 16.5 10.5 16.5H8" strokeLinecap="round"/>
            <path d="M8 16.5H10.5C11.9 16.5 13 15.4 13 14V11.5C13 10.1 11.9 9 10.5 9H5.5C4.1 9 3 7.9 3 6.5V4C3 2.6 4.1 1.5 5.5 1.5H8" strokeLinecap="round"/>
            <circle cx="5.5" cy="4.5" r="0.75" fill="currentColor"/>
            <circle cx="10.5" cy="13.5" r="0.75" fill="currentColor"/>
          </>
        );
      case "js":
      case "mjs":
      case "cjs":
        return (
          <>
            <rect x="1" y="1" width="14" height="14" rx="3" fill="currentColor" fillOpacity="0.1" strokeWidth="1.2"/>
            <text x="8" y="11" fill="currentColor" fontFamily="system-ui, -apple-system, sans-serif" fontSize="7.5" fontWeight="800" textAnchor="middle" letterSpacing="-0.2">JS</text>
          </>
        );
      case "jsx":
        return (
          <>
            <ellipse cx="8" cy="8" rx="7" ry="2.5" transform="rotate(30 8 8)" />
            <ellipse cx="8" cy="8" rx="7" ry="2.5" transform="rotate(150 8 8)" />
            <circle cx="8" cy="8" r="1" fill="currentColor"/>
          </>
        );
      case "json":
        return (
          <>
            <path d="M5 2C5 2 3.5 2 3.5 3.5V6.5C3.5 7.5 2.5 8 2.5 8C2.5 8 3.5 8.5 3.5 9.5V12.5C3.5 14 5 14 5 14" />
            <path d="M11 2C11 2 12.5 2 12.5 3.5V6.5C12.5 7.5 13.5 8 13.5 8C13.5 8 12.5 8.5 12.5 9.5V12.5C12.5 14 11 14 11 14" />
          </>
        );
      case "toml":
        return (
          <>
            <path d="M4.5 2.5H2.5V13.5H4.5" />
            <path d="M11.5 2.5H13.5V13.5H11.5" />
            <path d="M6 5.5H10" />
            <path d="M8 5.5V11.5" />
          </>
        );
      case "yaml":
      case "yml":
        return (
          <>
            <path d="M2 3H6" />
            <path d="M2 8H6" />
            <path d="M8 8H14" />
            <path d="M8 13H14" />
            <path d="M8 3V13" />
          </>
        );
      case "html":
      case "htm":
        return (
          <>
            <path d="M4.5 4.5L1 8L4.5 11.5" />
            <path d="M11.5 4.5L15 8L11.5 11.5" />
            <line x1="9.5" y1="3.5" x2="6.5" y2="12.5" />
          </>
        );
      case "sh":
      case "bash":
      case "zsh":
        return (
          <>
            <path d="M2.5 3.5L7 8L2.5 12.5" />
            <line x1="8.5" y1="12" x2="14.5" y2="12" />
          </>
        );
      case "txt":
      case "log":
        return (
          <>
            <line x1="2" y1="3.5" x2="14" y2="3.5" />
            <line x1="2" y1="7.5" x2="11" y2="7.5" />
            <line x1="2" y1="11.5" x2="13" y2="11.5" />
          </>
        );
      case "md":
      case "mdx":
        return (
          <>
            <rect x="1.5" y="2.5" width="13" height="11" rx="2" />
            <path d="M4 6V10M4 6L6 8L8 6M8 6V10" />
            <path d="M11 6V9M11 9H10M11 9H12" strokeWidth="1.2"/>
            <path d="M11 10L11 9.5" />
          </>
        );
      case "pdf":
        return (
          <>
            <rect x="2" y="1.5" width="12" height="13" rx="1.5" />
            <path d="M5.5 5H8C8.82843 5 9.5 5.67157 9.5 6.5C9.5 7.32843 8.82843 8 8 8H5.5V11" strokeWidth="1.3"/>
          </>
        );
      case "svg":
        return (
          <>
            <circle cx="2.5" cy="13.5" r="1" />
            <line x1="2.5" y1="12.5" x2="4" y2="9" strokeDasharray="1.5 1.5" />
            <circle cx="4.5" cy="8" r="1" fill="currentColor" />
            <path d="M2.5 13.5C5.5 11.5 10.5 4.5 13.5 2.5" strokeWidth="1.4" />
            <circle cx="13.5" cy="2.5" r="1" />
            <line x1="13.5" y1="3.5" x2="12" y2="7" strokeDasharray="1.5 1.5" />
            <circle cx="11.5" cy="8" r="1" fill="currentColor" />
          </>
        );
      case "go":
        return (
          <>
            <path d="M11.5 5.5C11 3.5 9.5 2.5 7.5 2.5C4.5 2.5 2.5 4.5 2.5 8C2.5 11.5 4.5 13.5 7.5 13.5C10 13.5 11.5 12 11.5 10H7.5" />
            <path d="M9.5 5.5H13.5" strokeWidth="1" />
            <path d="M9.5 7.5H13.5" strokeWidth="1" />
          </>
        );
      case "env":
        return (
          <>
            <circle cx="5.5" cy="10.5" r="3" />
            <path d="M7.62 8.38L12.5 3.5" />
            <path d="M10.5 5.5L12 7" />
            <path d="M12.5 3.5L13.5 4.5" />
          </>
        );
      default:
        if (meta.glyph === "image") {
          return (
            <>
              <rect x="1.5" y="1.5" width="13" height="13" rx="2" />
              <circle cx="5.5" cy="5.5" r="1.2" fill="currentColor" stroke="none" />
              <path d="M2 12.5L6.5 8L11 12.5" />
              <path d="M8.5 10.5L11.5 7.5L14.5 10.5" />
            </>
          );
        }
        return (
          <>
            <rect x="2" y="1.5" width="12" height="13" rx="1.5" />
            <text x="8" y="10.5" fill="currentColor" fontFamily="var(--font-code)" fontSize="6" fontWeight="700" textAnchor="middle">
              {meta.label.toLowerCase()}
            </text>
          </>
        );
    }
  };

  return (
    <svg
      className={`file-type-icon ${meta.tone || "plain"}`}
      viewBox="0 0 16 16"
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ color: meta.color } as React.CSSProperties}
      aria-hidden="true"
      focusable="false"
    >
      <title>{filename}</title>
      {renderSvgBody()}
    </svg>
  );
}

export function getCommandIcon() {
  return (
    <span className="file-type-icon command" aria-hidden="true">
      <TerminalSquare size={12} />
    </span>
  );
}
