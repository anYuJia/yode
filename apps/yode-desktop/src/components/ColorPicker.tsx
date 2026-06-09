import React, { useState, useRef, useEffect } from "react";

interface ColorPickerProps {
  value: string;
  onChange: (value: string) => void;
  className?: string;
  style?: React.CSSProperties;
}

export function ColorPicker({ value, onChange, className = "", style }: ColorPickerProps) {
  const [isOpen, setIsOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const saturationRef = useRef<HTMLDivElement>(null);
  const hueRef = useRef<HTMLDivElement>(null);

  // Parse hex to HSV
  const hexToHsv = (hex: string) => {
    let r = 0, g = 0, b = 0;
    const cleanHex = hex.replace("#", "");
    if (cleanHex.length === 3) {
      r = parseInt(cleanHex[0] + cleanHex[0], 16) / 255;
      g = parseInt(cleanHex[1] + cleanHex[1], 16) / 255;
      b = parseInt(cleanHex[2] + cleanHex[2], 16) / 255;
    } else if (cleanHex.length === 6) {
      r = parseInt(cleanHex.substring(0, 2), 16) / 255;
      g = parseInt(cleanHex.substring(2, 4), 16) / 255;
      b = parseInt(cleanHex.substring(4, 6), 16) / 255;
    }

    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const d = max - min;
    let h = 0;
    const s = max === 0 ? 0 : d / max;
    const v = max;

    if (max !== min) {
      switch (max) {
        case r:
          h = (g - b) / d + (g < b ? 6 : 0);
          break;
        case g:
          h = (b - r) / d + 2;
          break;
        case b:
          h = (r - g) / d + 4;
          break;
      }
      h /= 6;
    }
    return { h: h * 360, s: s * 100, v: v * 100 };
  };

  // Convert HSV back to Hex
  const hsvToHex = (h: number, s: number, v: number) => {
    h /= 360;
    s /= 100;
    v /= 100;

    let r = 0, g = 0, b = 0;
    const i = Math.floor(h * 6);
    const f = h * 6 - i;
    const p = v * (1 - s);
    const q = v * (1 - f * s);
    const t = v * (1 - (1 - f) * s);

    switch (i % 6) {
      case 0: r = v; g = t; b = p; break;
      case 1: r = q; g = v; b = p; break;
      case 2: r = p; g = v; b = t; break;
      case 3: r = p; g = q; b = v; break;
      case 4: r = t; g = p; b = v; break;
      case 5: r = v; g = p; b = q; break;
    }

    const toHex = (c: number) => {
      const hex = Math.round(c * 255).toString(16);
      return hex.length === 1 ? "0" + hex : hex;
    };
    return `#${toHex(r)}${toHex(g)}${toHex(b)}`.toUpperCase();
  };

  // Keep internal HSV representation
  const [hsv, setHsv] = useState(() => hexToHsv(value));

  // Sync HSV when value changes externally
  useEffect(() => {
    const nextHsv = hexToHsv(value);
    setHsv((prev) => {
      if (Math.abs(prev.h - nextHsv.h) > 1 || Math.abs(prev.s - nextHsv.s) > 1 || Math.abs(prev.v - nextHsv.v) > 1) {
        return nextHsv;
      }
      return prev;
    });
  }, [value]);

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

  const handleSaturationMouseDown = (e: React.MouseEvent) => {
    const move = (moveEvent: MouseEvent) => {
      if (!saturationRef.current) return;
      const rect = saturationRef.current.getBoundingClientRect();
      const x = Math.max(0, Math.min(rect.width, moveEvent.clientX - rect.left));
      const y = Math.max(0, Math.min(rect.height, moveEvent.clientY - rect.top));
      const s = (x / rect.width) * 100;
      const v = (1 - y / rect.height) * 100;
      setHsv((prev) => {
        const next = { ...prev, s, v };
        onChange(hsvToHex(next.h, next.s, next.v));
        return next;
      });
    };
    const up = () => {
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mouseup", up);
    };
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
    // Trigger once
    const rect = saturationRef.current!.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const y = Math.max(0, Math.min(rect.height, e.clientY - rect.top));
    const s = (x / rect.width) * 100;
    const v = (1 - y / rect.height) * 100;
    setHsv((prev) => {
      const next = { ...prev, s, v };
      onChange(hsvToHex(next.h, next.s, next.v));
      return next;
    });
  };

  const handleHueMouseDown = (e: React.MouseEvent) => {
    const move = (moveEvent: MouseEvent) => {
      if (!hueRef.current) return;
      const rect = hueRef.current.getBoundingClientRect();
      const x = Math.max(0, Math.min(rect.width, moveEvent.clientX - rect.left));
      const h = (x / rect.width) * 360;
      setHsv((prev) => {
        const next = { ...prev, h };
        onChange(hsvToHex(next.h, next.s, next.v));
        return next;
      });
    };
    const up = () => {
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mouseup", up);
    };
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
    // Trigger once
    const rect = hueRef.current!.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const h = (x / rect.width) * 360;
    setHsv((prev) => {
      const next = { ...prev, h };
      onChange(hsvToHex(next.h, next.s, next.v));
      return next;
    });
  };

  const baseHueColor = hsvToHex(hsv.h, 100, 100);

  return (
    <div
      ref={wrapperRef}
      className={`color-picker-wrapper ${className}`}
      style={{
        position: "relative",
        display: "flex",
        alignItems: "center",
        background: "var(--field)",
        border: "1px solid var(--line-soft)",
        borderRadius: "var(--radius)",
        padding: "0 8px",
        height: "28px",
        width: "110px",
        ...style
      }}
    >
      <button
        type="button"
        className="color-preview-btn"
        onClick={() => setIsOpen(!isOpen)}
        style={{
          width: "14px",
          height: "14px",
          borderRadius: "50%",
          backgroundColor: value,
          marginRight: "8px",
          border: "1px solid var(--line-soft)",
          cursor: "pointer",
          flexShrink: 0
        }}
      />
      <input
        type="text"
        className="text-input color-text"
        value={value}
        onChange={(e) => {
          const val = e.target.value;
          onChange(val);
          if (/^#[0-9A-F]{6}$/i.test(val) || /^#[0-9A-F]{3}$/i.test(val)) {
            setHsv(hexToHsv(val));
          }
        }}
        style={{
          border: "none",
          background: "transparent",
          color: "var(--text)",
          fontSize: "12px",
          outline: "none",
          width: "100%",
          fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace"
        }}
      />

      {isOpen && (
        <div
          className="color-picker-popover"
          style={{
            position: "absolute",
            bottom: "auto",
            top: "calc(100% + 6px)",
            right: 0,
            zIndex: 110,
            background: "var(--panel-raised)",
            border: "1px solid var(--line)",
            borderRadius: "var(--radius)",
            boxShadow: "0 10px 30px rgba(0, 0, 0, 0.4)",
            width: "220px",
            padding: "8px",
            display: "flex",
            flexDirection: "column",
            gap: "8px"
          }}
        >
          {/* Saturation/Value Board */}
          <div
            ref={saturationRef}
            onMouseDown={handleSaturationMouseDown}
            style={{
              position: "relative",
              height: "130px",
              borderRadius: "4px",
              background: `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, ${baseHueColor})`,
              cursor: "crosshair",
              overflow: "hidden"
            }}
          >
            <div
              style={{
                position: "absolute",
                left: `${hsv.s}%`,
                top: `${100 - hsv.v}%`,
                width: "12px",
                height: "12px",
                border: "2px solid #fff",
                borderRadius: "50%",
                boxShadow: "0 0 2px rgba(0,0,0,0.5)",
                transform: "translate(-6px, -6px)",
                pointerEvents: "none"
              }}
            />
          </div>

          {/* Hue Strip Slider */}
          <div
            ref={hueRef}
            onMouseDown={handleHueMouseDown}
            style={{
              position: "relative",
              height: "12px",
              borderRadius: "999px",
              background: "linear-gradient(to right, #ff0000, #ffff00, #00ff00, #00ffff, #0000ff, #ff00ff, #ff0000)",
              cursor: "ew-resize"
            }}
          >
            <div
              style={{
                position: "absolute",
                left: `${(hsv.h / 360) * 100}%`,
                top: "-1px",
                width: "14px",
                height: "14px",
                backgroundColor: "#fff",
                borderRadius: "50%",
                boxShadow: "0 1px 3px rgba(0,0,0,0.3)",
                transform: "translate(-7px, 0)",
                pointerEvents: "none",
                border: "1.5px solid var(--accent)"
              }}
            />
          </div>
        </div>
      )}
    </div>
  );
}
