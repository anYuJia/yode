import React, { useState, useEffect, useRef } from "react";
import { TerminalSquare, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

export function TerminalDrawer({ isOpen, onClose }: { isOpen: boolean; onClose: () => void }) {
  const [lines, setLines] = useState<string[]>([
    "Yode Terminal Session Started. Ready to execute local tasks."
  ]);
  const [inputValue, setInputValue] = useState("");
  const [isRunning, setIsRunning] = useState(false);
  const bodyElRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (bodyElRef.current) {
      bodyElRef.current.scrollTop = bodyElRef.current.scrollHeight;
    }
  }, [lines, isOpen]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputValue.trim() || isRunning) return;
    const cmd = inputValue.trim();
    setLines((prev) => [...prev, cmd]);
    setInputValue("");

    if (cmd === "clear") {
      setLines([]);
      return;
    }

    setIsRunning(true);
    try {
      const output = await invoke<string>("terminal_run", { command: cmd });
      setLines((prev) => [...prev, ...output.split("\n").map((line) => `  ${line}`)]);
    } catch (err) {
      setLines((prev) => [...prev, `  ${String(err)}`]);
    } finally {
      setIsRunning(false);
    }
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
          const isUserCommand = idx > 0 && !line.startsWith(" ");
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
            disabled={isRunning}
            placeholder={isRunning ? "running..." : ""}
            autoFocus={isOpen}
          />
        </form>
      </div>
    </div>
  );
}
