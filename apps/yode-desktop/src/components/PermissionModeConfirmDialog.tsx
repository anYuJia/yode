import React, { useEffect, useRef } from "react";
import { AlertTriangle } from "lucide-react";
import { createPortal } from "react-dom";

type PermissionModeConfirmDialogProps = {
  appLang: string;
  error: string | null;
  isSubmitting: boolean;
  onCancel: () => void;
  onConfirm: () => void;
};

const FOCUSABLE_SELECTOR = [
  "button:not([disabled])",
  "[href]",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])"
].join(",");

function focusableElements(dialog: HTMLElement) {
  return Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
}

/**
 * 完全信任是高风险、仅限当前应用会话的权限提升。
 * 该对话框故意不复用浏览器 confirm，以便提供可访问的焦点管理与失败重试状态。
 */
export function PermissionModeConfirmDialog({
  appLang,
  error,
  isSubmitting,
  onCancel,
  onConfirm
}: PermissionModeConfirmDialogProps) {
  const isZh = appLang === "zh";
  const dialogRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const cancelRefLatest = useRef(onCancel);
  const submittingRef = useRef(isSubmitting);

  useEffect(() => {
    cancelRefLatest.current = onCancel;
    submittingRef.current = isSubmitting;
  }, [isSubmitting, onCancel]);

  useEffect(() => {
    previousFocusRef.current = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const frame = window.requestAnimationFrame(() => {
      cancelRef.current?.focus();
    });

    const onKeyDown = (event: KeyboardEvent) => {
      const dialog = dialogRef.current;
      if (!dialog) return;
      if (event.key === "Escape" && !submittingRef.current) {
        event.preventDefault();
        cancelRefLatest.current();
        return;
      }
      if (event.key !== "Tab") return;

      const controls = focusableElements(dialog);
      if (controls.length === 0) {
        event.preventDefault();
        dialog.focus();
        return;
      }
      const currentIndex = controls.indexOf(document.activeElement as HTMLElement);
      const nextIndex = event.shiftKey
        ? (currentIndex <= 0 ? controls.length - 1 : currentIndex - 1)
        : (currentIndex === controls.length - 1 ? 0 : currentIndex + 1);
      event.preventDefault();
      controls[nextIndex]?.focus();
    };

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("keydown", onKeyDown);
      previousFocusRef.current?.focus();
    };
  }, []);

  const dialog = (
    <div
      className="settings-modal-backdrop permission-mode-modal-backdrop"
      onMouseDown={() => {
        if (!submittingRef.current) cancelRefLatest.current();
      }}
    >
      <div
        className="settings-modal permission-mode-confirmation"
        ref={dialogRef}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="permission-mode-confirmation-title"
        aria-describedby="permission-mode-confirmation-description"
        aria-busy={isSubmitting}
        tabIndex={-1}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="settings-modal-header">
          <div>
            <h2 id="permission-mode-confirmation-title">
              <AlertTriangle aria-hidden="true" size={18} />
              {isZh ? "启用完全信任？" : "Enable full access?"}
            </h2>
            <p id="permission-mode-confirmation-description">
              {isZh
                ? "此操作会跳过常规权限确认。"
                : "This skips normal permission confirmations."}
            </p>
          </div>
        </div>
        <div className="permission-mode-confirmation-body">
          <p>
            {isZh
              ? "完全信任仅在当前应用会话内生效，关闭应用后会恢复默认权限。危险命令保护仍会保留。"
              : "Full access applies only to this app session and resets when the app closes. Destructive-command protection remains enabled."}
          </p>
          <p>
            {isZh
              ? "仅当你了解将要执行的操作，并愿意承担跳过每次确认的风险时，才继续。"
              : "Continue only if you understand the planned actions and accept the risk of skipping per-action approval."}
          </p>
          {error ? <p className="permission-mode-confirmation-error" role="alert">{error}</p> : null}
        </div>
        <div className="permission-mode-confirmation-actions">
          <button
            ref={cancelRef}
            className="ghost-button"
            type="button"
            disabled={isSubmitting}
            onClick={onCancel}
          >
            {isZh ? "取消" : "Cancel"}
          </button>
          <button
            className="primary-button permission-mode-confirmation-confirm"
            type="button"
            disabled={isSubmitting}
            onClick={onConfirm}
          >
            {isSubmitting
              ? (isZh ? "正在启用..." : "Enabling...")
              : (isZh ? "我已了解并启用" : "I understand, enable")}
          </button>
        </div>
      </div>
    </div>
  );

  return createPortal(dialog, document.body);
}
