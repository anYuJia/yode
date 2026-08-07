import React, { useState, useEffect, useRef } from "react";
import { Check, CornerDownLeft, ShieldQuestion, TerminalSquare, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { TimelineItem } from "../../lib/desktopTypes";
import {
  PermissionDecision,
  permissionKeyAllowed,
  submitPermissionDecision
} from "../../lib/permissionActions";

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
      id: "allow_once" as PermissionDecision,
      label: isZh ? "允许本次执行" : "Yes, allow this time",
      description: isZh ? "仅允许本次执行" : "Only allow this execution"
    },
    {
      id: "always_allow" as PermissionDecision,
      label: isZh ? "总是允许此命令" : "Yes, always allow this command",
      description: isZh ? "后续同类命令不再询问" : "Do not ask again for similar commands"
    },
    {
      id: "deny" as PermissionDecision,
      label: isZh ? "拒绝并改用其他方式" : "No",
      description: isZh ? "告诉 agent 改用其他方式" : "Tell agent to use another way"
    }
  ];

  const [selectedIndex, setSelectedIndex] = useState(0);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const selectedOption = options[selectedIndex];
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const panelRef = useRef<HTMLDivElement | null>(null);

  const respond = async (decision: PermissionDecision) => {
    setIsSubmitting(true);
    setSubmitError(null);
    // 只有后端 RPC 成功后才移除权限卡片；失败保留卡片并显示可重试错误
    const result = await submitPermissionDecision({
      sessionId: item.sessionId,
      turnId: item.turnId,
      decision,
      submit: async (args) => {
        await invoke("permission_respond", args);
      }
    });
    if (result === "ok") {
      onResolved?.();
      return;
    }
    setIsSubmitting(false);
    setSubmitError(
      result === "missing-info"
        ? (isZh ? "缺少权限请求信息，无法提交。" : "Missing permission request info.")
        : (isZh ? "提交权限决定失败，请重试。" : "Failed to submit decision. Please retry.")
    );
  };

  useEffect(() => {
    setSelectedIndex(0);
    setSubmitError(null);
  }, [item.id]);

  useEffect(() => {
    optionRefs.current[selectedIndex]?.focus();
  }, [selectedIndex, item.id]);

  // 键盘操作只在焦点位于确认面板内时生效，避免普通输入框/终端/其他控件
  // 的 Enter 触发批准。Esc 拒绝当前请求。
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const panel = panelRef.current;
      if (!permissionKeyAllowed(e.target, panel)) return;
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((index) => (index - 1 + options.length) % options.length);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((index) => (index + 1) % options.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        void respond(selectedOption.id);
      } else if (e.key === "Escape") {
        e.preventDefault();
        void respond("deny");
      }
    };

    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [selectedOption.id, item.sessionId, item.turnId, item.id]);

  return (
    <div
      className="permission-prompt"
      ref={panelRef}
      role="dialog"
      aria-modal="false"
      aria-labelledby="permission-prompt-title"
    >
      <div className="permission-prompt-title">
        <span className="permission-prompt-icon">
          <ShieldQuestion size={17} />
        </span>
        <span id="permission-prompt-title">{isZh ? "允许运行此命令吗？" : "Allow running this command?"}</span>
        <button
          className="permission-esc-hint"
          onClick={() => void respond("deny")}
          type="button"
          title={isZh ? "按 Esc 拒绝" : "Press Esc to deny"}
        >
          <X size={13} />
          <span>{isZh ? "Esc 拒绝" : "Esc deny"}</span>
        </button>
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
              void respond(option.id);
            }}
            type="button"
            disabled={isSubmitting}
          >
            <kbd>{selectedIndex === index ? <Check size={13} /> : index + 1}</kbd>
            <span>{option.label}</span>
            <em>{option.description}</em>
          </button>
        ))}
      </div>
      {submitError ? (
        <p className="permission-error" role="alert">
          {submitError}
        </p>
      ) : null}
      <div className="permission-prompt-footer">
        <button
          className="permission-skip"
          onClick={() => void respond("deny")}
          type="button"
          disabled={isSubmitting}
        >
          {isZh ? "跳过" : "Skip"}
        </button>
        <button
          className="permission-submit"
          onClick={() => void respond(selectedOption.id)}
          type="button"
          disabled={isSubmitting}
        >
          {isSubmitting
            ? (isZh ? "提交中..." : "Submitting...")
            : (isZh ? "提交" : "Submit")}
          <CornerDownLeft size={14} />
        </button>
      </div>
    </div>
  );
}
