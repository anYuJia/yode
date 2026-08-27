import React, { useState, useRef, useMemo, useEffect, useLayoutEffect } from "react";
import {
  Paperclip,
  Folder,
  Check,
  FolderPlus,
  Hand,
  Shield,
  AlertCircle,
  ChevronDown,
  Send,
  Square,
  X
} from "lucide-react";
import { PROVIDERS_META } from "./settings/ProvidersSettings";
import { TopbarProviderIcon } from "./Topbar";
import { ImageAttachment } from "../lib/desktopTypes";
import { completeSlashCommands, slashCommandRegistry } from "../lib/localSlashCommands";
import {
  LLM_PROVIDERS_CHANGE_EVENT,
  modelsForProviderFromStorage
} from "../lib/llmProviderStorage";

const MAX_IMAGE_ATTACHMENTS = 8;
const MAX_IMAGE_BYTES = 10 * 1024 * 1024;
const MAX_IMAGE_PIXELS = 24_000_000;
const MAX_TOTAL_IMAGE_BYTES = 24 * 1024 * 1024;
const MAX_TOTAL_IMAGE_PIXELS = 48_000_000;

interface ComposerProps {
  draft: string;
  onDraftChange: (value: string) => void;
  images: ImageAttachment[];
  onImagesChange: (images: ImageAttachment[]) => void;
  onSendMessage: () => void;
  isProcessing: boolean;
  onCancelMessage: () => void;
  permissionMode: string;
  onPermissionModeChange: (mode: string) => Promise<boolean>;
  permissionModeUpdating: boolean;
  permissionModeError: string | null;
  appLang: string;
  projectOptions: Array<{ label: string; root: string | null }>;
  selectedProjectRoot: string | null;
  onProjectRootChange: (root: string | null) => void;
  onAddProject: () => Promise<void>;
  currentProvider: string;
  currentModel: string;
  onModelChange: (model: string) => void;
  showBottomPanel: boolean;
  showContextUsage: boolean;
  requireOptEnter: boolean;
}

export function Composer({
  draft,
  onDraftChange,
  images,
  onImagesChange,
  onSendMessage,
  isProcessing,
  onCancelMessage,
  permissionMode,
  onPermissionModeChange,
  permissionModeUpdating,
  permissionModeError,
  appLang,
  projectOptions,
  selectedProjectRoot,
  onProjectRootChange,
  onAddProject,
  currentProvider,
  currentModel,
  onModelChange,
  showBottomPanel,
  showContextUsage,
  requireOptEnter
}: ComposerProps) {
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [projectDropdownOpen, setProjectDropdownOpen] = useState(false);
  const [modelDropdownOpen, setModelDropdownOpen] = useState(false);
  const [isDraggingImage, setIsDraggingImage] = useState(false);
  const [attachmentNotice, setAttachmentNotice] = useState("");
  const [providerVersion, setProviderVersion] = useState(0);
  const [slashSuggestions, setSlashSuggestions] = useState<string[]>([]);
  const [slashHighlight, setSlashHighlight] = useState(0);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const projectDropdownRef = useRef<HTMLDivElement>(null);
  const modelDropdownRef = useRef<HTMLDivElement>(null);
  const permissionTriggerRef = useRef<HTMLButtonElement>(null);
  const projectTriggerRef = useRef<HTMLButtonElement>(null);
  const modelTriggerRef = useRef<HTMLButtonElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const isZh = appLang === "zh";
  const canSend = draft.trim().length > 0 || images.length > 0;

  const modelOptions = useMemo(() => {
    return modelsForProviderFromStorage(currentProvider, PROVIDERS_META);
  }, [currentProvider, providerVersion]);

  useEffect(() => {
    const refreshProviders = () => setProviderVersion((version) => version + 1);
    window.addEventListener("storage", refreshProviders);
    window.addEventListener(LLM_PROVIDERS_CHANGE_EVENT, refreshProviders);
    return () => {
      window.removeEventListener("storage", refreshProviders);
      window.removeEventListener(LLM_PROVIDERS_CHANGE_EVENT, refreshProviders);
    };
  }, []);

  const OPTIONS = [
    {
      key: "default",
      label: isZh ? "每次询问" : "Ask for approval",
      description: isZh ? "修改外部文件及使用网络时，总是需要确认" : "Always ask to edit external files and use the internet",
      icon: <Hand size={15} />
    },
    {
      key: "auto",
      label: isZh ? "自动授权安全操作" : "Approve for me",
      description: isZh ? "仅对检测到存在潜在风险的操作进行询问" : "Only ask for actions detected as potentially unsafe",
      icon: <Shield size={15} />
    },
    {
      key: "bypass",
      label: isZh ? "完全信任" : "Full access",
      description: isZh ? "不再弹出权限确认，仍保留危险命令保护" : "Skip permission prompts while keeping destructive-command protection",
      icon: <AlertCircle size={15} />
    }
  ];

  /** slash 命令自动补全：由统一 registry 驱动（解析 + 补全 + 帮助共用一份定义）。 */
  const updateSlashSuggestions = (value: string) => {
    const trimmed = value.trim();
    const hasSpace = /\s/.test(trimmed.slice(1));
    if (trimmed.startsWith("/") && !hasSpace) {
      const suggestions = trimmed === "/"
        ? Object.keys(slashCommandRegistry).sort().map((name) => `/${name}`)
        : completeSlashCommands(trimmed);
      setSlashSuggestions(suggestions);
      setSlashHighlight(0);
    } else {
      setSlashSuggestions([]);
    }
  };

  const applySlashSuggestion = (suggestion: string) => {
    onDraftChange(`${suggestion} `);
    setSlashSuggestions([]);
    setSlashHighlight(0);
    textareaRef.current?.focus();
  };

  const currentOption = OPTIONS.find(
    (o) => o.key.toLowerCase() === (permissionMode || "default").toLowerCase()
  ) || OPTIONS[0];
  const currentProject =
    selectedProjectRoot === null
      ? projectOptions.find((option) => option.root === null) ?? {
          label: isZh ? "独立对话" : "Standalone",
          root: null
        }
      : projectOptions.find((option) => option.root === selectedProjectRoot) ??
        projectOptions[0] ?? {
          label: isZh ? "当前项目" : "Current project",
          root: selectedProjectRoot ?? null
        };

  const handlePopupNavigation = (
    event: React.KeyboardEvent,
    isOpen: boolean,
    setOpen: React.Dispatch<React.SetStateAction<boolean>>,
    popupRef: React.RefObject<HTMLDivElement>
  ) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const focusItem = (position: "first" | "last" | "next" | "previous") => {
      const items = Array.from(
        popupRef.current?.querySelectorAll<HTMLButtonElement>(
          '[role="option"]:not([disabled]), [role="menuitem"]:not([disabled]), [role="menuitemradio"]:not([disabled])'
        ) ?? []
      );
      if (items.length === 0) return;
      const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);
      const targetIndex = position === "first"
        ? 0
        : position === "last"
          ? items.length - 1
          : position === "next"
            ? currentIndex < 0 ? 0 : (currentIndex + 1) % items.length
            : currentIndex < 0
              ? items.length - 1
              : (currentIndex - 1 + items.length) % items.length;
      items[targetIndex]?.focus();
    };

    if (!isOpen) {
      setOpen(true);
      window.requestAnimationFrame(() => {
        focusItem(event.key === "ArrowUp" || event.key === "End" ? "last" : "first");
      });
      return;
    }
    if (event.key === "Home") focusItem("first");
    else if (event.key === "End") focusItem("last");
    else focusItem(event.key === "ArrowDown" ? "next" : "previous");
  };

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setDropdownOpen(false);
      }
      if (
        projectDropdownRef.current &&
        !projectDropdownRef.current.contains(event.target as Node)
      ) {
        setProjectDropdownOpen(false);
      }
      if (
        modelDropdownRef.current &&
        !modelDropdownRef.current.contains(event.target as Node)
      ) {
        setModelDropdownOpen(false);
      }
    }
    if (dropdownOpen || projectDropdownOpen || modelDropdownOpen) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [dropdownOpen, projectDropdownOpen, modelDropdownOpen]);

  useLayoutEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    const maxHeight = window.innerHeight <= 720 ? 64 : window.innerHeight <= 768 ? 84 : 144;
    textarea.style.height = "0px";
    const nextHeight = Math.max(44, Math.min(textarea.scrollHeight, maxHeight));
    textarea.style.height = `${nextHeight}px`;
    textarea.style.overflowY = textarea.scrollHeight > maxHeight ? "auto" : "hidden";
  }, [draft]);

  const addImageFiles = async (files: FileList | File[]) => {
    const allFiles = Array.from(files);
    const imageFiles = allFiles.filter((file) => file.type.startsWith("image/"));
    if (imageFiles.length === 0) return;
    const availableSlots = Math.max(0, MAX_IMAGE_ATTACHMENTS - images.length);
    if (availableSlots === 0) {
      setAttachmentNotice(isZh ? `最多可添加 ${MAX_IMAGE_ATTACHMENTS} 张图片。` : `You can attach up to ${MAX_IMAGE_ATTACHMENTS} images.`);
      return;
    }

    const acceptedFiles = imageFiles
      .filter((file) => file.size <= MAX_IMAGE_BYTES)
      .slice(0, availableSlots);
    const skippedTooLarge = imageFiles.length - imageFiles.filter((file) => file.size <= MAX_IMAGE_BYTES).length;
    const skippedTooMany = Math.max(0, imageFiles.filter((file) => file.size <= MAX_IMAGE_BYTES).length - availableSlots);

    if (acceptedFiles.length === 0) {
      setAttachmentNotice(isZh ? "图片过大，单张图片不能超过 10MB。" : "Images are too large. Each image must be 10MB or smaller.");
      return;
    }

    const existingBytes = images.reduce((sum, image) => sum + (image.size || 0), 0);
    const existingPixels = images.reduce((sum, image) => sum + (image.width || 0) * (image.height || 0), 0);
    const next: ImageAttachment[] = [];
    let totalBytes = existingBytes;
    let totalPixels = existingPixels;
    let skippedBudget = 0;
    for (const file of acceptedFiles) {
      if (totalBytes + file.size > MAX_TOTAL_IMAGE_BYTES) {
        skippedBudget += 1;
        continue;
      }
      try {
        const attachment = await fileToImageAttachment(file);
        const pixels = (attachment.width || 0) * (attachment.height || 0);
        if (pixels > MAX_IMAGE_PIXELS || totalPixels + pixels > MAX_TOTAL_IMAGE_PIXELS) {
          skippedBudget += 1;
          continue;
        }
        next.push(attachment);
        totalBytes += file.size;
        totalPixels += pixels;
      } catch {
        skippedBudget += 1;
      }
    }
    if (next.length > 0) onImagesChange([...images, ...next]);
    if (skippedTooLarge > 0 || skippedTooMany > 0 || skippedBudget > 0) {
      setAttachmentNotice(
        isZh
          ? [
              skippedTooLarge > 0 ? `${skippedTooLarge} 张图片超过 10MB，已跳过。` : "",
              skippedTooMany > 0 ? `最多可添加 ${MAX_IMAGE_ATTACHMENTS} 张图片，超出的已跳过。` : "",
              skippedBudget > 0 ? `${skippedBudget} 张图片超过像素或附件总预算，已跳过。` : ""
            ].filter(Boolean).join(" ")
          : [
              skippedTooLarge > 0 ? `${skippedTooLarge} image(s) exceeded 10MB and were skipped.` : "",
              skippedTooMany > 0 ? `Only ${MAX_IMAGE_ATTACHMENTS} images can be attached; extra images were skipped.` : "",
              skippedBudget > 0 ? `${skippedBudget} image(s) exceeded the pixel or total attachment budget and were skipped.` : ""
            ].filter(Boolean).join(" ")
      );
    } else {
      setAttachmentNotice("");
    }
  };

  return (
    <footer
      className={`composer ${isDraggingImage ? "dragging-image" : ""}`}
      style={{ position: "relative" }}
      onKeyDownCapture={(event) => {
        if (event.key !== "Escape") return;
        if (dropdownOpen || projectDropdownOpen || modelDropdownOpen) {
          const returnTarget = projectDropdownOpen
            ? projectTriggerRef.current
            : dropdownOpen
              ? permissionTriggerRef.current
              : modelTriggerRef.current;
          setDropdownOpen(false);
          setProjectDropdownOpen(false);
          setModelDropdownOpen(false);
          event.preventDefault();
          event.stopPropagation();
          window.requestAnimationFrame(() => returnTarget?.focus());
        }
      }}
      onDragEnter={(event) => {
        if (Array.from(event.dataTransfer.items).some((item) => item.type.startsWith("image/"))) {
          event.preventDefault();
          setIsDraggingImage(true);
        }
      }}
      onDragOver={(event) => {
        if (Array.from(event.dataTransfer.items).some((item) => item.type.startsWith("image/"))) {
          event.preventDefault();
        }
      }}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          setIsDraggingImage(false);
        }
      }}
      onDrop={(event) => {
        setIsDraggingImage(false);
        const files = Array.from(event.dataTransfer.files).filter((file) =>
          file.type.startsWith("image/")
        );
        if (files.length > 0) {
          event.preventDefault();
          void addImageFiles(files);
        }
      }}
    >
      {images.length > 0 && (
        <div className="composer-attachments" aria-label={isZh ? "图片附件" : "Image attachments"}>
          {images.map((image) => (
            <div className="composer-image-chip" key={image.id} title={image.name}>
              <img src={image.dataUrl} alt={image.name} />
              <span>{image.name}</span>
              <button
                type="button"
                className="composer-image-remove"
                title={isZh ? "移除图片" : "Remove image"}
                aria-label={isZh ? `移除图片 ${image.name}` : `Remove image ${image.name}`}
                onClick={() => onImagesChange(images.filter((item) => item.id !== image.id))}
              >
                <X size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
      {attachmentNotice && (
        <div className="composer-attachment-notice" role="status" aria-live="polite">
          {attachmentNotice}
        </div>
      )}
      <textarea
        id="message-composer"
        aria-label={isZh ? "消息" : "Message"}
        aria-describedby="composer-keyboard-hint"
        aria-autocomplete="list"
        aria-controls={slashSuggestions.length > 0 ? "composer-slash-listbox" : undefined}
        aria-expanded={slashSuggestions.length > 0}
        aria-activedescendant={slashSuggestions.length > 0 ? `composer-slash-option-${slashHighlight}` : undefined}
        ref={textareaRef}
        rows={1}
        autoFocus
        placeholder={isZh ? "描述任务，输入 / 查看命令…" : "Describe a task, or type / for commands…"}
        value={draft}
        onChange={(event) => {
          onDraftChange(event.target.value);
          updateSlashSuggestions(event.target.value);
        }}
        onPaste={(event) => {
          const files = Array.from(event.clipboardData.files).filter((file) =>
            file.type.startsWith("image/")
          );
          if (files.length > 0) {
            event.preventDefault();
            void addImageFiles(files);
          }
        }}
        onKeyDown={(event) => {
          if (slashSuggestions.length > 0) {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              setSlashHighlight((current) => (current + 1) % slashSuggestions.length);
              return;
            }
            if (event.key === "ArrowUp") {
              event.preventDefault();
              setSlashHighlight(
                (current) => (current - 1 + slashSuggestions.length) % slashSuggestions.length
              );
              return;
            }
            if (event.key === "Home" || event.key === "End") {
              event.preventDefault();
              setSlashHighlight(event.key === "Home" ? 0 : slashSuggestions.length - 1);
              return;
            }
            if (event.key === "Tab" || event.key === "Enter") {
              event.preventDefault();
              applySlashSuggestion(slashSuggestions[slashHighlight] ?? slashSuggestions[0]);
              return;
            }
            if (event.key === "Escape") {
              event.preventDefault();
              setSlashSuggestions([]);
              return;
            }
          }
          if (event.key === "Enter" && !event.shiftKey) {
            if (requireOptEnter) {
              if (event.altKey) {
                event.preventDefault();
                onSendMessage();
              }
              return;
            }
            if (event.metaKey || event.ctrlKey) {
              event.preventDefault();
              const target = event.target as HTMLTextAreaElement;
              const start = target.selectionStart;
              const end = target.selectionEnd;
              const val = target.value;
              const nextVal = val.substring(0, start) + "\n" + val.substring(end);
              onDraftChange(nextVal);
              setTimeout(() => {
                target.selectionStart = target.selectionEnd = start + 1;
              }, 0);
            } else {
              event.preventDefault();
              onSendMessage();
            }
          }
        }}
      />
      {slashSuggestions.length > 0 ? (
        <div id="composer-slash-listbox" className="context-dropdown slash-suggestions" role="listbox" aria-label="命令补全">
          {slashSuggestions.map((suggestion, index) => (
            <button
              id={`composer-slash-option-${index}`}
              key={suggestion}
              type="button"
              role="option"
              aria-selected={index === slashHighlight}
              className={`context-option ${index === slashHighlight ? "selected" : ""}`}
              onMouseEnter={() => setSlashHighlight(index)}
              onClick={() => applySlashSuggestion(suggestion)}
            >
              <span>{suggestion}</span>
            </button>
          ))}
        </div>
      ) : null}
      <div className="composer-toolbar">
        {showBottomPanel ? (
        <div className="composer-tools" style={{ position: "relative" }}>
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            multiple
            style={{ display: "none" }}
            onChange={(event) => {
              const files = event.currentTarget.files;
              if (files) void addImageFiles(files);
              event.currentTarget.value = "";
            }}
          />
          <button
            className="icon-button"
            type="button"
            title={isZh ? "添加图片" : "Attach image"}
            aria-label={isZh ? "添加图片" : "Attach image"}
            onClick={() => fileInputRef.current?.click()}
            style={{ outline: "none", boxShadow: "none" }}
          >
            <Paperclip size={17} />
          </button>

          <div
            ref={projectDropdownRef}
            style={{ display: "inline-block", position: "relative" }}
            onKeyDown={(event) => handlePopupNavigation(
              event,
              projectDropdownOpen,
              setProjectDropdownOpen,
              projectDropdownRef
            )}
          >
            <button
              ref={projectTriggerRef}
              className="mode-chip"
              type="button"
              onClick={() => setProjectDropdownOpen(!projectDropdownOpen)}
              title={currentProject.root ?? (isZh ? "独立对话" : "Standalone")}
              aria-label={`${isZh ? "项目" : "Project"}：${currentProject.label}`}
              aria-haspopup="menu"
              aria-expanded={projectDropdownOpen}
              aria-controls="composer-project-listbox"
              style={{ outline: "none", boxShadow: "none", cursor: "pointer" }}
            >
              <Folder size={15} />
              {currentProject.label}
              <ChevronDown size={11} className={projectDropdownOpen ? "is-open" : ""} />
            </button>

            {projectDropdownOpen && (
              <div id="composer-project-listbox" className="context-dropdown project-dropdown" role="menu" aria-label={isZh ? "选择项目" : "Choose project"}>
                {projectOptions.map((option) => {
                  const selected = option.root === selectedProjectRoot;
                  return (
                    <button
                      key={option.root ?? "__standalone__"}
                      type="button"
                      role="menuitemradio"
                      aria-checked={selected}
                      className={`context-option ${selected ? "selected" : ""}`}
                      onClick={() => {
                        onProjectRootChange(option.root);
                        setProjectDropdownOpen(false);
                        window.requestAnimationFrame(() => projectTriggerRef.current?.focus());
                      }}
                    >
                      <Folder size={14} />
                      <span>{option.label}</span>
                      {selected ? <Check size={14} /> : null}
                    </button>
                  );
                })}
                <div className="context-dropdown-divider" role="separator" />
                <button
                  type="button"
                  role="menuitem"
                  className="context-option context-option-action"
                  onClick={() => {
                    setProjectDropdownOpen(false);
                    window.requestAnimationFrame(() => projectTriggerRef.current?.focus());
                    void onAddProject();
                  }}
                >
                  <FolderPlus size={14} />
                  <span>{isZh ? "添加项目..." : "Add project..."}</span>
                </button>
              </div>
            )}
          </div>
          
          <div
            ref={dropdownRef}
            style={{ display: "inline-block" }}
            onKeyDown={(event) => handlePopupNavigation(
              event,
              dropdownOpen,
              setDropdownOpen,
              dropdownRef
            )}
          >
            <button
              ref={permissionTriggerRef}
              className="mode-chip"
              type="button"
              onClick={() => setDropdownOpen(!dropdownOpen)}
              disabled={permissionModeUpdating}
              aria-haspopup="dialog"
              aria-expanded={dropdownOpen}
              aria-controls="composer-permission-popup"
              aria-busy={permissionModeUpdating}
              aria-label={`${isZh ? "权限模式" : "Permission mode"}：${currentOption.label}`}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: "6px",
                cursor: "pointer",
                position: "relative",
                outline: "none",
                boxShadow: "none"
              }}
            >
              {currentOption.icon}
              {currentOption.label}
              <ChevronDown size={11} className={dropdownOpen ? "is-open" : ""} />
            </button>

            {dropdownOpen && (
              <div
                id="composer-permission-popup"
                className="permission-dropdown"
                role="dialog"
                aria-label={isZh ? "选择权限模式" : "Choose permission mode"}
                style={{
                  position: "absolute",
                  bottom: "100%",
                  left: "0",
                  marginBottom: "8px",
                  zIndex: 1000,
                  width: "380px",
                  background: "var(--panel)",
                  border: "1px solid var(--line)",
                  borderRadius: "8px",
                  boxShadow: "0 4px 20px rgba(0, 0, 0, 0.3)",
                  padding: "16px",
                  display: "flex",
                  flexDirection: "column",
                  gap: "12px"
                }}
              >
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center"
                  }}
                >
                  <span
                    style={{
                      fontSize: "12px",
                      color: "var(--text-soft)",
                      fontWeight: 500
                    }}
                  >
                    {isZh ? "如何授权 Yode 的操作？" : "How should Yode actions be approved?"}
                  </span>
                  <span className="permission-dropdown-note">
                    {isZh ? "可随时切换" : "Change anytime"}
                  </span>
                </div>

                <div
                  id="composer-permission-listbox"
                  role="listbox"
                  aria-label={isZh ? "权限模式" : "Permission mode"}
                  style={{ display: "flex", flexDirection: "column", gap: "8px" }}
                >
                  {OPTIONS.map((option) => {
                    const isSelected = option.key.toLowerCase() === currentOption.key.toLowerCase();
                    return (
                      <button
                        key={option.key}
                        type="button"
                        role="option"
                        aria-selected={isSelected}
                        disabled={permissionModeUpdating}
                        onClick={() => {
                          void onPermissionModeChange(option.key).then((accepted) => {
                            if (accepted) {
                              setDropdownOpen(false);
                              window.requestAnimationFrame(() => permissionTriggerRef.current?.focus());
                            }
                          });
                        }}
                        style={{
                          display: "flex",
                          alignItems: "flex-start",
                          gap: "12px",
                          width: "100%",
                          padding: "10px",
                          background: isSelected ? "rgba(255, 255, 255, 0.05)" : "transparent",
                          border: "none",
                          borderRadius: "6px",
                          textAlign: "left",
                          cursor: "pointer",
                          transition: "background 0.2s",
                          outline: "none",
                          boxShadow: "none"
                        }}
                        className="dropdown-option-btn"
                      >
                        <div style={{ marginTop: "2px", color: isSelected ? "var(--accent)" : "var(--text-soft)" }}>
                           {option.icon}
                        </div>
                        <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: "2px" }}>
                          <span style={{ fontSize: "13px", fontWeight: 500, color: "var(--text)" }}>
                            {option.label}
                          </span>
                          <span style={{ fontSize: "11px", color: "var(--text-soft)", lineHeight: "1.4" }}>
                            {option.description}
                          </span>
                        </div>
                        {isSelected && (
                          <Check size={14} style={{ color: "var(--accent)", alignSelf: "center" }} />
                        )}
                      </button>
                    );
                  })}
                </div>
                {permissionModeError ? (
                  <p className="permission-error" role="alert">
                    {permissionModeError}
                  </p>
                ) : null}
              </div>
            )}
          </div>

          <div
            ref={modelDropdownRef}
            style={{ display: "inline-block", position: "relative" }}
            onKeyDown={(event) => handlePopupNavigation(
              event,
              modelDropdownOpen,
              setModelDropdownOpen,
              modelDropdownRef
            )}
          >
            <button
              ref={modelTriggerRef}
              className="mode-chip"
              type="button"
              onClick={() => setModelDropdownOpen(!modelDropdownOpen)}
              aria-label={`${isZh ? "模型" : "Model"}：${currentModel || (isZh ? "未选择" : "Not selected")}`}
              aria-haspopup={modelOptions.length > 0 ? "listbox" : undefined}
              aria-expanded={modelDropdownOpen}
              aria-controls="composer-model-listbox"
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: "6px",
                cursor: "pointer",
                outline: "none",
                boxShadow: "none"
              }}
            >
              <TopbarProviderIcon id={currentProvider} />
              <span>{currentModel || (isZh ? "选择模型" : "Select model")}</span>
              <ChevronDown size={11} style={{ opacity: 0.7, transform: modelDropdownOpen ? "rotate(180deg)" : "none", transition: "transform 150ms" }} />
            </button>

            {modelDropdownOpen && (
              <div
                id="composer-model-listbox"
                className="context-dropdown model-dropdown"
                role={modelOptions.length > 0 ? "listbox" : "status"}
                aria-label={modelOptions.length > 0 ? (isZh ? "选择模型" : "Choose model") : undefined}
                aria-live={modelOptions.length === 0 ? "polite" : undefined}
              >
                {modelOptions.map((model: string) => {
                  const selected = model === currentModel;
                  return (
                    <button
                      key={model}
                      type="button"
                      role="option"
                      aria-selected={selected}
                      className={`context-option ${selected ? "selected" : ""}`}
                      onClick={() => {
                        onModelChange(model);
                        setModelDropdownOpen(false);
                        window.requestAnimationFrame(() => modelTriggerRef.current?.focus());
                      }}
                    >
                      <TopbarProviderIcon id={currentProvider} />
                      <span>{model}</span>
                      {selected ? <Check size={14} style={{ color: "var(--accent)" }} /> : <span />}
                    </button>
                  );
                })}
                {modelOptions.length === 0 ? (
                  <div className="context-dropdown-empty">
                    {currentProvider
                      ? (isZh ? "请先在设置中为此提供商添加模型" : "Add a model for this provider in Settings first")
                      : (isZh ? "请先从顶部选择模型提供商" : "Choose a model provider from the top bar first")}
                  </div>
                ) : null}
              </div>
            )}
          </div>
        </div>
        ) : <div />}
        <div className="composer-actions">
          <span className="composer-keyboard-hint" id="composer-keyboard-hint">
            {requireOptEnter
              ? (isZh ? "⌥ Enter 发送" : "⌥ Enter to send")
              : (isZh ? "Enter 发送 · Shift Enter 换行" : "Enter to send · Shift Enter for new line")}
          </span>
          {showContextUsage ? (
            <span className="context-usage-chip" title={isZh ? "当前输入估算用量" : "Estimated current input usage"}>
              {isZh ? `${draft.length.toLocaleString()} 字` : `${draft.length.toLocaleString()} chars`}
            </span>
          ) : null}
          {isProcessing ? (
            <button 
              className="send-button stop-button" 
              onClick={onCancelMessage} 
              type="button" 
              title={isZh ? "终止" : "Stop"} 
              aria-label={isZh ? "终止当前运行" : "Stop current run"}
              style={{ 
                background: "transparent", 
                border: "none", 
                color: "var(--error)", 
                outline: "none", 
                boxShadow: "none",
                display: "inline-grid",
                placeItems: "center",
                transition: "color 0.15s ease",
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = "transparent";
                e.currentTarget.style.color = "color-mix(in oklch, var(--error), var(--text) 20%)";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = "transparent";
                e.currentTarget.style.color = "var(--error)";
              }}
            >
              <Square size={13} fill="currentColor" style={{ borderRadius: "1px" }} />
            </button>
          ) : (
            <button
              className="send-button"
              onClick={onSendMessage}
              type="button"
              title={isZh ? "发送" : "Send"}
              aria-label={isZh ? "发送消息" : "Send message"}
              disabled={!canSend}
              style={{ outline: "none", boxShadow: "none" }}
            >
              <span className="sr-only">{isZh ? "发送消息" : "Send message"}</span>
              <Send size={17} />
            </button>
          )}
        </div>
      </div>
    </footer>
  );
}

function fileToImageAttachment(file: File): Promise<ImageAttachment> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("Failed to read image"));
    reader.onload = () => {
      const dataUrl = String(reader.result ?? "");
      const base64 = dataUrl.includes(",") ? dataUrl.split(",", 2)[1] : dataUrl;
      const finish = (width: number, height: number) => resolve({
        id: `${Date.now()}-${crypto.randomUUID?.() ?? Math.random().toString(36).slice(2)}`,
        name: file.name || "image",
        mediaType: file.type || "image/png",
        base64,
        dataUrl,
        size: file.size,
        width,
        height
      });
      const image = new Image();
      image.onload = () => {
        const width = image.naturalWidth || image.width;
        const height = image.naturalHeight || image.height;
        image.src = "";
        if (!width || !height) {
          reject(new Error("无法解析图片尺寸"));
          return;
        }
        finish(width, height);
      };
      image.onerror = () => {
        image.src = "";
        reject(new Error("无法解析图片尺寸"));
      };
      image.src = dataUrl;
    };
    reader.readAsDataURL(file);
  });
}
