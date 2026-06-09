import React, { useState } from "react";
import { Search, Trash2 } from "lucide-react";

export interface ArchivedChatInfo {
  id: string;
  title: string;
  date: string;
  project: string;
}

export function ArchivedChatsSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [searchQuery, setSearchQuery] = useState("");
  const [chats, setChats] = useState<ArchivedChatInfo[]>(() => {
    const saved = localStorage.getItem("yode-archived-chats");
    if (saved) {
      try {
        return JSON.parse(saved);
      } catch (e) {
        // use defaults
      }
    }
    return [
      { id: "1", title: "排查 Rust 下载串视频问题", date: "2026年6月5日, 16:39", project: "douyin" },
      { id: "2", title: "评估好友聊天功能支持", date: "2026年6月5日, 14:44", project: "douyin" },
      { id: "3", title: "修复 CI/CD Clippy 报错", date: "2026年6月5日, 9:32", project: "douyin" },
      { id: "4", title: "提交并推送到服务器", date: "2026年6月5日, 9:01", project: "douyin" },
      { id: "5", title: "检查项目差异与UI优化", date: "2026年6月4日, 15:41", project: "douyin" },
      { id: "6", title: "解析 get_user_message 请求", date: "2026年6月4日, 14:07", project: "douyin" },
      { id: "7", title: "hi", date: "2026年6月4日, 9:05", project: "douyin" },
      { id: "8", title: "拉取服务器最新版本", date: "2026年6月3日, 11:22", project: "douyin" },
      { id: "9", title: "优化菜单栏结构", date: "2026年6月3日, 9:45", project: "lh" },
      { id: "10", title: "创建仓库并提交推送", date: "2026年5月28日, 14:06", project: "lh" },
      { id: "11", title: "审阅 analysis_results 报告", date: "2026年5月28日, 10:20", project: "yode" }
    ];
  });

  const saveChats = (list: ArchivedChatInfo[]) => {
    setChats(list);
    localStorage.setItem("yode-archived-chats", JSON.stringify(list));
  };

  const handleUnarchive = (id: string, title: string) => {
    const updated = chats.filter(c => c.id !== id);
    saveChats(updated);
    alert(t(`对话 "${title}" 已成功取消归档，并已放回主会话列表中！`, `Chat "${title}" has been unarchived and restored to the main sessions list!`));
  };

  const handleDelete = (id: string, title: string) => {
    if (confirm(t(`确定要永久删除已归档对话 "${title}" 吗？此操作无法撤销。`, `Are you sure you want to permanently delete archived chat "${title}"? This action cannot be undone.`))) {
      const updated = chats.filter(c => c.id !== id);
      saveChats(updated);
    }
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

      {/* Main chats container */}
      <div className="theme-card" style={{ padding: "8px 0", maxHeight: "68vh", overflowY: "auto" }}>
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
                  {/* Delete Button */}
                  <button
                    onClick={() => handleDelete(chat.id, chat.title)}
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
                </div>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
