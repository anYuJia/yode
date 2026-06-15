import React, { useState, useEffect, useRef } from "react";
import { Check, CornerDownLeft, ShieldQuestion, TerminalSquare } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { TimelineItem } from "../../lib/mock";

export function PermissionActions({
  item,
  appLang,
  onResolved
}: {
  item: Extract<TimelineItem, { kind: "permission" }>;
  appLang: string;
  onResolved?: () => void;
}) {
  const isZh = appLang === "zh";

  const options = [
    {
      id: "allow_once",
      label: isZh ? "允许本次执行" : "Yes, allow this time",
      description: isZh ? "仅允许本次执行" : "Only allow this execution"
    },
    {
      id: "always_allow",
      label: isZh ? "总是允许此命令" : "Yes, always allow this command",
      description: isZh ? "后续同类命令不再询问" : "Do not ask again for similar commands"
    },
    {
      id: "deny",
      label: isZh ? "拒绝并改用其他方式" : "No",
      description: isZh ? "告诉 agent 改用其他方式" : "Tell agent to use another way"
    }
  ] as const;

  const [selectedIndex, setSelectedIndex] = useState(0);
  const selectedOption = options[selectedIndex];
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const respond = (decision: (typeof options)[number]["id"]) => {
    onResolved?.();
    if (item.sessionId && item.turnId) {
      invoke("permission_respond", {
        sessionId: item.sessionId,
        turnId: item.turnId,
        allow: decision !== "deny",
        alwaysAllow: decision === "always_allow"
      }).catch(console.error);
    }
  };

  useEffect(() => {
    setSelectedIndex(0);
  }, [item.id]);

  useEffect(() => {
    optionRefs.current[selectedIndex]?.focus();
  }, [selectedIndex, item.id]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((index) => (index - 1 + options.length) % options.length);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((index) => (index + 1) % options.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        respond(selectedOption.id);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedOption.id, item.sessionId, item.turnId]);

  return (
    <div className="permission-prompt">
      <div className="permission-prompt-title">
        <span className="permission-prompt-icon">
          <ShieldQuestion size={17} />
        </span>
        <span>{isZh ? "允许运行此命令吗？" : "Allow running this command?"}</span>
      </div>
      <div className="permission-command-shell">
        <TerminalSquare size={14} />
        <pre className="permission-command">{item.body || item.tool}</pre>
      </div>
      <div className="permission-option-list">
        {options.map((option, index) => (
          <button
            className={`permission-option ${selectedIndex === index ? "selected" : ""}`}
            key={option.id}
            ref={(node) => {
              optionRefs.current[index] = node;
            }}
            onClick={() => {
              setSelectedIndex(index);
              respond(option.id);
            }}
            type="button"
            style={{ outline: "none", boxShadow: "none" }}
          >
            <kbd>{selectedIndex === index ? <Check size={13} /> : index + 1}</kbd>
            <span>{option.label}</span>
            <em>{option.description}</em>
          </button>
        ))}
      </div>
      <div className="permission-prompt-footer">
        <button className="permission-skip" onClick={() => respond("deny")} type="button" style={{ outline: "none", boxShadow: "none" }}>
          {isZh ? "跳过" : "Skip"}
        </button>
        <button className="permission-submit" onClick={() => respond(selectedOption.id)} type="button" style={{ outline: "none", boxShadow: "none" }}>
          {isZh ? "提交" : "Submit"}
          <CornerDownLeft size={14} />
        </button>
      </div>
    </div>
  );
}
