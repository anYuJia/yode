import React, { useState, useEffect, useRef } from "react";
import { TerminalSquare, X } from "lucide-react";

export function TerminalDrawer({ isOpen, onClose }: { isOpen: boolean; onClose: () => void }) {
  const [lines, setLines] = useState<string[]>([
    "Yode Terminal Session Started. Ready to execute local tasks.",
    "cargo test -p yode-core",
    "Running 12 tests...",
    "test test_version_compare ... ok",
    "test test_session_db ... ok",
    "test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
  ]);
  const [inputValue, setInputValue] = useState("");
  const bodyElRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (bodyElRef.current) {
      bodyElRef.current.scrollTop = bodyElRef.current.scrollHeight;
    }
  }, [lines, isOpen]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputValue.trim()) return;
    const cmd = inputValue.trim();
    setLines((prev) => [...prev, cmd]);
    setInputValue("");

    // Add mock response
    setTimeout(() => {
      if (cmd.startsWith("help")) {
        setLines((prev) => [
          ...prev,
          "  Available commands: help, clear, status, cargo build, git status"
        ]);
      } else if (cmd.startsWith("clear")) {
        setLines([]);
      } else if (cmd.startsWith("cargo build")) {
        setLines((prev) => [
          ...prev,
          "   Compiling yode-core v0.1.0",
          "   Compiling yode-desktop v0.0.19",
          "    Finished dev profile in 1.42s"
        ]);
      } else if (cmd.startsWith("status")) {
        setLines((prev) => [
          ...prev,
          "  Agent Engine Status: Idle",
          "  Workspace: active",
          "  Connections: localhost:1420"
        ]);
      } else if (cmd.startsWith("git status")) {
        setLines((prev) => [
          ...prev,
          "  On branch main",
          "  Your branch is up to date with 'origin/main'.",
          "  nothing to commit, working tree clean"
        ]);
      } else {
        setLines((prev) => [
          ...prev,
          `  bash: ${cmd.split(" ")[0]}: command not found. Type 'help' for options.`
        ]);
      }
    }, 150);
  };

  return (
    <div className={`terminal-drawer ${isOpen ? "open" : ""}`}>
      <div className="terminal-header">
        <div className="terminal-title">
          <TerminalSquare size={13} />
          <span>终端 (Terminal) - bash</span>
        </div>
        <button className="close-btn" onClick={onClose} type="button" title="关闭终端">
          <X size={14} />
        </button>
      </div>
      <div className="terminal-body" ref={bodyElRef}>
        {lines.map((line, idx) => {
          // Identify if it was entered as a command
          const isUserCommand = idx === 1 || (idx > 5 && !line.startsWith(" ") && !line.startsWith("Running") && !line.startsWith("test") && !line.startsWith("  bash:") && !line.startsWith("  Available") && !line.startsWith("  Agent") && !line.startsWith("  Workspace:") && !line.startsWith("  Connections:") && !line.startsWith("  On branch") && !line.startsWith("  Your branch") && !line.startsWith("  nothing"));
          return (
            <div key={idx} className={`terminal-line ${line.startsWith("test") || line.includes("Finished") ? "text-success" : line.includes("command not found") ? "text-error" : ""}`}>
              {isUserCommand && <span className="terminal-prompt">yode@local:~/yode$</span>}
              {line}
            </div>
          );
        })}
        <form onSubmit={handleSubmit} className="terminal-line active-line">
          <span className="terminal-prompt">yode@local:~/yode$</span>
          <input
            type="text"
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            className="terminal-real-input"
            autoFocus={isOpen}
          />
        </form>
      </div>
    </div>
  );
}
