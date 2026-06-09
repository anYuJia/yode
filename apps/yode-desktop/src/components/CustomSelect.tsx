import React, { useState, useRef, useEffect } from "react";
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

export function CustomSelect({ value, onChange, options, className = "", style }: CustomSelectProps) {
  const [isOpen, setIsOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);

  const selectedOption = options.find((opt) => opt.value === value) || options[0];

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (wrapperRef.current && !wrapperRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, []);

  return (
    <div
      ref={wrapperRef}
      className={`custom-select-wrapper ${className}`}
      style={{ position: "relative", display: "inline-block", ...style }}
    >
      <button
        type="button"
        className="custom-select-trigger"
        onClick={() => setIsOpen(!isOpen)}
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
          className="custom-select-dropdown"
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
          {options.map((option) => {
            const isSelected = option.value === value;
            return (
              <button
                key={option.value}
                type="button"
                onClick={() => {
                  onChange(option.value);
                  setIsOpen(false);
                }}
                className={`custom-select-option ${isSelected ? "selected" : ""}`}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  width: "100%",
                  padding: "6px 8px",
                  border: "none",
                  background: isSelected ? "color-mix(in oklch, var(--accent-muted), transparent 40%)" : "transparent",
                  color: isSelected ? "var(--text)" : "var(--text-muted)",
                  borderRadius: "calc(var(--radius) - 2px)",
                  fontSize: "12px",
                  cursor: "pointer",
                  textAlign: "left",
                  transition: "background 100ms, color 100ms"
                }}
                onMouseEnter={(e) => {
                  if (!isSelected) {
                    e.currentTarget.style.background = "var(--field)";
                    e.currentTarget.style.color = "var(--text)";
                  }
                }}
                onMouseLeave={(e) => {
                  if (!isSelected) {
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
