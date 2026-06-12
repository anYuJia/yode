import React, { useState, useRef, useMemo, useEffect } from "react";
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
import { ImageAttachment } from "../lib/mock";

const MAX_IMAGE_ATTACHMENTS = 8;
const MAX_IMAGE_BYTES = 10 * 1024 * 1024;

interface ComposerProps {
  draft: string;
  onDraftChange: (value: string) => void;
  images: ImageAttachment[];
  onImagesChange: (images: ImageAttachment[]) => void;
  onSendMessage: () => void;
  isProcessing: boolean;
  onCancelMessage: () => void;
  permissionMode: string;
  onPermissionModeChange: (mode: string) => void;
  appLang: string;
  projectOptions: Array<{ label: string; root: string | null }>;
  selectedProjectRoot: string | null;
  onProjectRootChange: (root: string | null) => void;
  onAddProject: () => Promise<void>;
  currentProvider: string;
  currentModel: string;
  onModelChange: (model: string) => void;
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
  appLang,
  projectOptions,
  selectedProjectRoot,
  onProjectRootChange,
  onAddProject,
  currentProvider,
  currentModel,
  onModelChange
}: ComposerProps) {
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [projectDropdownOpen, setProjectDropdownOpen] = useState(false);
  const [modelDropdownOpen, setModelDropdownOpen] = useState(false);
  const [isDraggingImage, setIsDraggingImage] = useState(false);
  const [attachmentNotice, setAttachmentNotice] = useState("");
  const dropdownRef = useRef<HTMLDivElement>(null);
  const projectDropdownRef = useRef<HTMLDivElement>(null);
  const modelDropdownRef = useRef<HTMLDivElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const isZh = appLang === "zh";

  const modelOptions = useMemo(() => {
    const saved = localStorage.getItem("yode-llm-providers");
    let list: any[] = [];
    if (saved) {
      try {
        const data = JSON.parse(saved);
        if (Array.isArray(data)) {
          list = data;
        } else if (data && typeof data === "object") {
          list = Object.values(data);
        }
      } catch (e) {}
    }
    const found = list.find((p: any) => p && p.id === currentProvider);
    if (found && Array.isArray(found.models) && found.models.length > 0) {
      return found.models;
    }
    const meta = PROVIDERS_META.find((p) => p.id === currentProvider);
    return meta ? meta.defaultModels : [];
  }, [currentProvider]);

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

    const next = await Promise.all(acceptedFiles.map(fileToImageAttachment));
    onImagesChange([...images, ...next]);
    if (skippedTooLarge > 0 || skippedTooMany > 0) {
      setAttachmentNotice(
        isZh
          ? [
              skippedTooLarge > 0 ? `${skippedTooLarge} 张图片超过 10MB，已跳过。` : "",
              skippedTooMany > 0 ? `最多可添加 ${MAX_IMAGE_ATTACHMENTS} 张图片，超出的已跳过。` : ""
            ].filter(Boolean).join(" ")
          : [
              skippedTooLarge > 0 ? `${skippedTooLarge} image(s) exceeded 10MB and were skipped.` : "",
              skippedTooMany > 0 ? `Only ${MAX_IMAGE_ATTACHMENTS} images can be attached; extra images were skipped.` : ""
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
                onClick={() => onImagesChange(images.filter((item) => item.id !== image.id))}
              >
                <X size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
      {attachmentNotice && (
        <div className="composer-attachment-notice" role="status">
          {attachmentNotice}
        </div>
      )}
      <textarea
        aria-label="消息"
        placeholder={isZh ? "输入仓库任务..." : "Enter repository task..."}
        value={draft}
        onChange={(event) => onDraftChange(event.target.value)}
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
          if (event.key === "Enter" && !event.shiftKey) {
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
      <div className="composer-toolbar">
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
            onClick={() => fileInputRef.current?.click()}
            style={{ outline: "none", boxShadow: "none" }}
          >
            <Paperclip size={17} />
          </button>

          <div ref={projectDropdownRef} style={{ display: "inline-block", position: "relative" }}>
            <button
              className="mode-chip"
              type="button"
              onClick={() => setProjectDropdownOpen(!projectDropdownOpen)}
              title={currentProject.root ?? (isZh ? "独立对话" : "Standalone")}
              style={{ outline: "none", boxShadow: "none", cursor: "pointer" }}
            >
              <Folder size={15} />
              {currentProject.label}
            </button>

            {projectDropdownOpen && (
              <div className="context-dropdown project-dropdown">
                {projectOptions.map((option) => {
                  const selected = option.root === selectedProjectRoot;
                  return (
                    <button
                      key={option.root ?? "__standalone__"}
                      type="button"
                      className={`context-option ${selected ? "selected" : ""}`}
                      onClick={() => {
                        onProjectRootChange(option.root);
                        setProjectDropdownOpen(false);
                      }}
                    >
                      <Folder size={14} />
                      <span>{option.label}</span>
                      {selected ? <Check size={14} /> : null}
                    </button>
                  );
                })}
                <div className="context-dropdown-divider" />
                <button
                  type="button"
                  className="context-option context-option-action"
                  onClick={() => {
                    setProjectDropdownOpen(false);
                    void onAddProject();
                  }}
                >
                  <FolderPlus size={14} />
                  <span>{isZh ? "添加项目..." : "Add project..."}</span>
                </button>
              </div>
            )}
          </div>
          
          <div ref={dropdownRef} style={{ display: "inline-block" }}>
            <button
              className="mode-chip"
              type="button"
              onClick={() => setDropdownOpen(!dropdownOpen)}
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
            </button>

            {dropdownOpen && (
              <div
                className="permission-dropdown"
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
                  <a
                    href="#"
                    onClick={(e) => e.preventDefault()}
                    style={{
                      fontSize: "12px",
                      color: "var(--text-soft)",
                      textDecoration: "underline"
                    }}
                  >
                    {isZh ? "了解更多" : "Learn more"}
                  </a>
                </div>

                <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                  {OPTIONS.map((option) => {
                    const isSelected = option.key.toLowerCase() === currentOption.key.toLowerCase();
                    return (
                      <button
                        key={option.key}
                        type="button"
                        onClick={() => {
                          onPermissionModeChange(option.key);
                          setDropdownOpen(false);
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
              </div>
            )}
          </div>

          <div ref={modelDropdownRef} style={{ display: "inline-block", position: "relative" }}>
            <button
              className="mode-chip"
              type="button"
              onClick={() => setModelDropdownOpen(!modelDropdownOpen)}
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
              <div className="context-dropdown model-dropdown">
                {modelOptions.map((model: string) => {
                  const selected = model === currentModel;
                  return (
                    <button
                      key={model}
                      type="button"
                      className={`context-option ${selected ? "selected" : ""}`}
                      onClick={() => {
                        onModelChange(model);
                        setModelDropdownOpen(false);
                      }}
                    >
                      <TopbarProviderIcon id={currentProvider} />
                      <span>{model}</span>
                      {selected ? <Check size={14} style={{ color: "var(--accent)" }} /> : <span />}
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </div>
        <div className="composer-actions">
          {isProcessing ? (
            <button 
              className="send-button stop-button" 
              onClick={onCancelMessage} 
              type="button" 
              title={isZh ? "终止" : "Stop"} 
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
            <button className="send-button" onClick={onSendMessage} type="button" title={isZh ? "发送" : "Send"} style={{ outline: "none", boxShadow: "none" }}>
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
      resolve({
        id: `${Date.now()}-${crypto.randomUUID?.() ?? Math.random().toString(36).slice(2)}`,
        name: file.name || "image",
        mediaType: file.type || "image/png",
        base64,
        dataUrl,
        size: file.size
      });
    };
    reader.readAsDataURL(file);
  });
}
