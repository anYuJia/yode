import React, { useState, useRef, useEffect, useId } from "react";
import { ChevronDown, Check } from "lucide-react";

export interface CustomSelectOption {
  value: string;
  label: string;
  avatarText?: string;
  avatarBg?: string;
  avatarFg?: string;
}

interface CustomSelectProps {
  value: string;
  onChange: (value: string) => void;
  options: CustomSelectOption[];
  className?: string;
  style?: React.CSSProperties;
}

type CustomSelectKeyboardAction =
  | { type: "highlight"; index: number }
  | { type: "select"; index: number }
  | { type: "close" }
  | { type: "tab" }
  | { type: "none" };

export function customSelectKeyboardAction(
  key: string,
  highlightIndex: number,
  optionCount: number
): CustomSelectKeyboardAction {
  if (key === "Escape") return { type: "close" };
  if (key === "Tab") return { type: "tab" };
  if (optionCount === 0) return { type: "none" };

  if (key === "ArrowDown") {
    return { type: "highlight", index: highlightIndex < 0 ? 0 : Math.min(highlightIndex + 1, optionCount - 1) };
  }
  if (key === "ArrowUp") {
    return { type: "highlight", index: highlightIndex < 0 ? optionCount - 1 : Math.max(highlightIndex - 1, 0) };
  }
  if (key === "Home") return { type: "highlight", index: 0 };
  if (key === "End") return { type: "highlight", index: optionCount - 1 };
  if ((key === "Enter" || key === " ") && highlightIndex >= 0 && highlightIndex < optionCount) {
    return { type: "select", index: highlightIndex };
  }
  return { type: "none" };
}

export function CustomSelect({ value, onChange, options, className = "", style }: CustomSelectProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [highlightIndex, setHighlightIndex] = useState(-1);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const listboxId = useId();

  const selectedOption = options.find((opt) => opt.value === value) || options[0];
  const activeOptionId = isOpen && highlightIndex >= 0 ? `${listboxId}-option-${highlightIndex}` : undefined;

  const close = (restoreFocus = true) => {
    setIsOpen(false);
    setHighlightIndex(-1);
    if (restoreFocus) triggerRef.current?.focus();
  };

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (wrapperRef.current && !wrapperRef.current.contains(event.target as Node)) {
        setIsOpen(false);
        setHighlightIndex(-1);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, []);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(event.target as Node)) return;

      const action = customSelectKeyboardAction(event.key, highlightIndex, options.length);
      if (action.type === "close") {
        event.preventDefault();
        close(true);
        return;
      }
      if (action.type === "tab") {
        close(false);
        return;
      }
      if (action.type === "highlight") {
        event.preventDefault();
        setHighlightIndex(action.index);
        scrollHighlightIntoView(action.index);
        return;
      }
      if (action.type === "select") {
        event.preventDefault();
        onChange(options[action.index].value);
        close(true);
        return;
      }
      // 输入字符时跳到首个匹配选项
      if (event.key.length === 1) {
        const needle = event.key.toLowerCase();
        const match = options.findIndex((option) => option.label.toLowerCase().startsWith(needle));
        if (match >= 0) {
          setHighlightIndex(match);
          scrollHighlightIntoView(match);
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [isOpen, options, highlightIndex]);

  const scrollHighlightIntoView = (index: number) => {
    const list = listRef.current;
    const optionEl = list?.querySelector<HTMLElement>(`[data-option-index="${index}"]`);
    optionEl?.scrollIntoView({ block: "nearest" });
  };

  return (
    <div
      ref={wrapperRef}
      className={`custom-select-wrapper ${className}`}
      style={{ position: "relative", display: "inline-block", ...style }}
    >
      <button
        ref={triggerRef}
        type="button"
        className="custom-select-trigger"
        onClick={() => {
          setIsOpen(!isOpen);
          if (!isOpen) setHighlightIndex(options.findIndex((option) => option.value === value));
        }}
        aria-haspopup="listbox"
        aria-expanded={isOpen}
        aria-controls={listboxId}
        aria-activedescendant={activeOptionId}
        aria-label={selectedOption?.label}
        style={{
          display: "flex",
          alignItems: "center",
          gap: "8px",
          background: "var(--field)",
          border: "1px solid var(--line-soft)",
          borderRadius: "var(--radius)",
          height: "28px",
          padding: selectedOption?.avatarText ? "0 28px 0 8px" : "0 28px 0 10px",
          position: "relative",
          cursor: "pointer",
          fontSize: "12px",
          color: "var(--text)",
          textAlign: "left",
          width: "100%",
          minWidth: "140px"
        }}
      >
        {selectedOption?.avatarText && (
          <span
            className="theme-avatar"
            style={{
              position: "static",
              fontSize: "11px",
              fontWeight: 700,
              background: selectedOption.avatarBg || "var(--accent-muted)",
              color: selectedOption.avatarFg || "var(--accent)",
              padding: "1px 4px",
              borderRadius: "3px",
              lineHeight: "1",
              userSelect: "none"
            }}
          >
            {selectedOption.avatarText}
          </span>
        )}
        <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {selectedOption?.label}
        </span>
        <ChevronDown
          size={14}
          className="select-arrow"
          style={{
            position: "absolute",
            right: "8px",
            color: "var(--text-soft)",
            transform: isOpen ? "rotate(180deg)" : "rotate(0deg)",
            transition: "transform 150ms ease"
          }}
        />
      </button>

      {isOpen && (
        <div
          ref={listRef}
          id={listboxId}
          className="custom-select-dropdown"
          role="listbox"
          aria-label="选项列表"
          style={{
            position: "absolute",
            bottom: "auto",
            top: "calc(100% + 4px)",
            right: 0,
            zIndex: 100,
            background: "var(--panel-raised)",
            border: "1px solid var(--line)",
            borderRadius: "var(--radius)",
            boxShadow: "0 10px 25px rgba(0, 0, 0, 0.4)",
            minWidth: "180px",
            maxHeight: "240px",
            overflowY: "auto",
            padding: "4px"
          }}
        >
          {options.map((option, index) => {
            const isSelected = option.value === value;
            const isHighlighted = highlightIndex === index;
            return (
              <button
                key={option.value}
                type="button"
                id={`${listboxId}-option-${index}`}
                data-option-index={index}
                role="option"
                aria-selected={isSelected}
                tabIndex={-1}
                onClick={() => {
                  onChange(option.value);
                  setIsOpen(false);
                  setHighlightIndex(-1);
                  triggerRef.current?.focus();
                }}
                className={`custom-select-option ${isSelected ? "selected" : ""} ${isHighlighted ? "highlighted" : ""}`}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  width: "100%",
                  padding: "6px 8px",
                  border: "none",
                  background: isHighlighted
                    ? "color-mix(in oklch, var(--accent-muted), transparent 20%)"
                    : isSelected
                      ? "color-mix(in oklch, var(--accent-muted), transparent 40%)"
                      : "transparent",
                  color: isSelected ? "var(--text)" : "var(--text-muted)",
                  borderRadius: "calc(var(--radius) - 2px)",
                  fontSize: "12px",
                  cursor: "pointer",
                  textAlign: "left",
                  transition: "background 100ms, color 100ms"
                }}
                onMouseEnter={(e) => {
                  setHighlightIndex(index);
                  if (!isSelected) {
                    e.currentTarget.style.background = "var(--field)";
                    e.currentTarget.style.color = "var(--text)";
                  }
                }}
                onMouseLeave={(e) => {
                  if (!isSelected && highlightIndex !== index) {
                    e.currentTarget.style.background = "transparent";
                    e.currentTarget.style.color = "var(--text-muted)";
                  }
                }}
              >
                {option.avatarText && (
                  <span
                    style={{
                      fontSize: "11px",
                      fontWeight: 700,
                      background: option.avatarBg || "var(--accent-muted)",
                      color: option.avatarFg || "var(--accent)",
                      padding: "1px 4px",
                      borderRadius: "3px",
                      lineHeight: "1"
                    }}
                  >
                    {option.avatarText}
                  </span>
                )}
                <span style={{ flex: 1 }}>{option.label}</span>
                {isSelected && <Check size={13} style={{ color: "var(--accent)", marginLeft: "auto" }} />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
