import React, { useState } from "react";
import { Search, Trash2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArchivedChatInfo,
  loadStoredArchivedChats,
  markArchivedSessionDeletedLocally,
  saveStoredArchivedChats,
  unarchiveSessionLocally
} from "../../lib/projectStorage";

export function ArchivedChatsSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [searchQuery, setSearchQuery] = useState("");
  const [deletingChatId, setDeletingChatId] = useState<string | null>(null);
  const [statusText, setStatusText] = useState("");
  const [chats, setChats] = useState<ArchivedChatInfo[]>(loadStoredArchivedChats);

  const saveChats = (list: ArchivedChatInfo[]) => {
    setChats(list);
    saveStoredArchivedChats(list);
  };

  const handleUnarchive = (id: string, title: string) => {
    setChats(unarchiveSessionLocally(id));
    window.dispatchEvent(new CustomEvent("yode-session-unarchived", { detail: { sessionId: id } }));

    setStatusText(t(`对话 "${title}" 已恢复。`, `Chat "${title}" restored.`));
  };

  const handleDelete = async (id: string, title: string) => {
    // Delete in the backend first. If it fails, keep a local tombstone so it does not reappear.
    if ("__TAURI_INTERNALS__" in window) {
      try {
        await invoke("sessions_delete", { sessionId: id, session_id: id });
      } catch (err) {
        console.error("Failed to delete session from database:", err);
      }
    }

    setChats(markArchivedSessionDeletedLocally(id));
    window.dispatchEvent(new CustomEvent("yode-session-deleted-permanently", { detail: { sessionId: id } }));
  };

  const filteredChats = chats.filter(c =>
    c.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
    c.project.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
      {/* Search Input */}
      <div style={{ position: "relative", width: "100%" }}>
        <Search size={13} style={{ position: "absolute", left: "10px", top: "9px", color: "var(--text-soft)", opacity: 0.8 }} />
        <input
          type="text"
          placeholder={t("搜索已归档对话...", "Search archived chats...")}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{
            width: "100%",
            height: "30px",
            background: "var(--field)",
            border: "1px solid var(--line-soft)",
            borderRadius: "var(--radius)",
            paddingLeft: "30px",
            paddingRight: "10px",
            fontSize: "12.5px",
            color: "var(--text)",
            outline: "none"
          }}
        />
      </div>
      {statusText && (
        <div style={{ fontSize: "11px", color: "var(--text-soft)" }}>
          {statusText}
        </div>
      )}

      {/* Main chats container */}
      <div className="theme-card" style={{ padding: "8px 0", maxHeight: "calc(100vh - 150px)", overflowY: "auto" }}>
        {filteredChats.length === 0 ? (
          <div style={{ paddingBlock: "32px", textAlign: "center", color: "var(--text-soft)", fontSize: "13px" }}>
            {t("没有找到归档的对话", "No archived chats found")}
          </div>
        ) : (
          filteredChats.map((chat, idx) => (
            <div key={chat.id}>
              {idx > 0 && <div className="divider" style={{ margin: "2px 16px" }} />}
              <div
                className="form-row"
                style={{
                  minHeight: "56px",
                  paddingInline: "16px",
                  paddingBlock: "8px"
                }}
              >
                {/* Title & metadata */}
                <div style={{ display: "flex", flexDirection: "column", gap: "3px" }}>
                  <span style={{ fontSize: "13.5px", fontWeight: "600", color: "var(--text)" }}>
                    {chat.title}
                  </span>
                  <span style={{ fontSize: "11px", color: "var(--text-soft)", opacity: 0.85 }}>
                    {chat.date} · <code style={{ fontFamily: "var(--font-code)", fontSize: "10.5px" }}>{chat.project}</code>
                  </span>
                </div>

                {/* Actions */}
                <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
                  {deletingChatId === chat.id ? (
                    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                      <button
                        onClick={() => {
                          handleDelete(chat.id, chat.title);
                          setDeletingChatId(null);
                        }}
                        type="button"
                        style={{
                          background: "oklch(60% 0.16 30)",
                          color: "#fff",
                          border: "none",
                          borderRadius: "4px",
                          padding: "4px 8px",
                          fontSize: "11px",
                          fontWeight: "600",
                          cursor: "pointer"
                        }}
                      >
                        {t("确认删除", "Confirm")}
                      </button>
                      <button
                        onClick={() => setDeletingChatId(null)}
                        type="button"
                        style={{
                          background: "transparent",
                          color: "var(--text-soft)",
                          border: "none",
                          cursor: "pointer",
                          fontSize: "11px"
                        }}
                      >
                        {t("取消", "Cancel")}
                      </button>
                    </div>
                  ) : (
                    <>
                      {/* Delete Button */}
                      <button
                        onClick={() => setDeletingChatId(chat.id)}
                        type="button"
                        style={{
                          background: "transparent",
                          border: "none",
                          cursor: "pointer",
                          color: "var(--text-soft)",
                          padding: "4px",
                          display: "flex",
                          alignItems: "center"
                        }}
                        onMouseEnter={(e) => e.currentTarget.style.color = "oklch(67% 0.15 28)"}
                        onMouseLeave={(e) => e.currentTarget.style.color = "var(--text-soft)"}
                      >
                        <Trash2 size={14} />
                      </button>

                      {/* Unarchive Button */}
                      <button
                        onClick={() => handleUnarchive(chat.id, chat.title)}
                        type="button"
                        className="secondary-button"
                        style={{
                          paddingInline: "14px",
                          height: "26px",
                          fontSize: "11.5px",
                          fontWeight: "600",
                          cursor: "pointer"
                        }}
                      >
                        {t("取消归档", "Unarchive")}
                      </button>
                    </>
                  )}
                </div>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
