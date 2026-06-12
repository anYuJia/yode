import React, { useState } from "react";
import { Search, SlidersHorizontal, X } from "lucide-react";

export function KeyboardShortcutsSettings({ isZh, t }: { isZh: boolean; t: (zh: string, en: string) => string }) {
  const [searchQuery, setSearchQuery] = useState("");

  const [bindings, setBindings] = useState<
    Array<{
      id: string;
      cmdZh: string;
      cmdEn: string;
      descZh: string;
      descEn: string;
      keys: string[];
    }>
  >([
    { id: "archive", cmdZh: "归档对话", cmdEn: "Archive chat", descZh: "归档当前活动的对话", descEn: "Archive the current chat", keys: ["⇧⌘A"] },
    { id: "newchat", cmdZh: "新建对话", cmdEn: "New chat", descZh: "发起一个新的对话", descEn: "Start a new chat", keys: ["⌘N", "⇧⌘O"] },
    { id: "sidechat", cmdZh: "打开侧边栏对话", cmdEn: "Open side chat", descZh: "在侧边栏中打开当前对话", descEn: "Open the current chat in a side chat", keys: [] },
    { id: "newwin", cmdZh: "在新窗口打开", cmdEn: "Open in new window", descZh: "在新窗口中打开当前对话", descEn: "Open the current chat in a new window", keys: [] },
    { id: "quickchat", cmdZh: "新建快速对话", cmdEn: "New quick chat", descZh: "在快速输入框中启动轻量对话", descEn: "Start a lightweight chat in the quick composer", keys: ["⌥⌘N"] },
    { id: "pin", cmdZh: "固定/取消固定", cmdEn: "Toggle pin", descZh: "固定或取消固定当前对话", descEn: "Pin or unpin the current chat", keys: ["⌥⌘P"] },
    { id: "find", cmdZh: "查找", cmdEn: "Find", descZh: "在当前对话中搜索内容", descEn: "Search the current chat", keys: ["⌘F"] },
    { id: "addressbar", cmdZh: "聚焦浏览器地址栏", cmdEn: "Focus browser address bar", descZh: "将焦点定位到应用内浏览器地址栏", descEn: "Focus the in-app browser address bar", keys: ["⌘L"] },
    { id: "back", cmdZh: "后退", cmdEn: "Back", descZh: "在导航历史记录中向后退一步", descEn: "Go back in navigation history", keys: ["⌘[", "Mouse Back"] },
    { id: "forward", cmdZh: "前进", cmdEn: "Forward", descZh: "在导航历史记录中向前进一步", descEn: "Go forward in navigation history", keys: ["⌘]", "Mouse Forward"] },
    { id: "next_chat_tab", cmdZh: "下一个对话或标签页", cmdEn: "Next chat or tab", descZh: "切换至下一个对话或标签页", descEn: "Switch to the next chat or tab", keys: ["⇧⌘]", "⌥⌘Right"] },
    { id: "prev_recent", cmdZh: "上一个最近查看的对话或标签页", cmdEn: "Previous recently viewed chat or tab", descZh: "轮转切换至上一个或最近查看的对话或标签页", descEn: "Cycle to the previous recently viewed chat or tab", keys: ["⌃⇧Tab"] },
    { id: "prev_chat_tab", cmdZh: "上一个对话或标签页", cmdEn: "Previous chat or tab", descZh: "切换至上一个对话或标签页", descEn: "Switch to the previous chat or tab", keys: ["⇧⌘[", "⌥⌘Left"] },
    { id: "open_browser_tab", cmdZh: "打开浏览器标签页", cmdEn: "Open browser tab", descZh: "打开一个新的浏览器标签页", descEn: "Open a browser tab", keys: ["⌘T"] },
    { id: "open_review_tab", cmdZh: "打开代码审查标签页", cmdEn: "Open review tab", descZh: "打开代码审查标签页", descEn: "Open the review tab", keys: ["⌃⇧G"] },
    { id: "toggle_bottom_panel", cmdZh: "显示/隐藏底部面板", cmdEn: "Toggle bottom panel", descZh: "显示或隐藏底部面板", descEn: "Show or hide the bottom panel", keys: ["⌘J"] },
    { id: "toggle_browser_panel", cmdZh: "显示/隐藏浏览器面板", cmdEn: "Toggle browser panel", descZh: "显示或隐藏浏览器面板", descEn: "Show or hide the browser panel", keys: ["⇧⌘B"] },
    { id: "toggle_sidebar", cmdZh: "显示/隐藏侧边栏", cmdEn: "Toggle sidebar", descZh: "显示或隐藏侧边栏", descEn: "Show or hide the sidebar", keys: ["⌘B"] },
    { id: "toggle_side_panel", cmdZh: "显示/隐藏侧栏面板", cmdEn: "Toggle side panel", descZh: "显示或隐藏侧栏面板", descEn: "Show or hide the side panel", keys: ["⌥⌘B"] },
    { id: "open_terminal", cmdZh: "打开终端", cmdEn: "Open terminal", descZh: "打开终端面板", descEn: "Open the terminal panel", keys: ["⌃`"] },
    { id: "env_action_1", cmdZh: "环境操作 1", cmdEn: "Environment action 1", descZh: "在此快捷键槽位中运行环境操作", descEn: "Run the environment action in this shortcut slot", keys: ["⇧⌘D"] },
    { id: "env_action_2", cmdZh: "环境操作 2", cmdEn: "Environment action 2", descZh: "在此快捷键槽位中运行环境操作", descEn: "Run the environment action in this shortcut slot", keys: [] },
    { id: "env_action_3", cmdZh: "环境操作 3", cmdEn: "Environment action 3", descZh: "在此快捷键槽位中运行环境操作", descEn: "Run the environment action in this shortcut slot", keys: [] },
    { id: "env_action_4", cmdZh: "环境操作 4", cmdEn: "Environment action 4", descZh: "在此快捷键槽位中运行环境操作", descEn: "Run the environment action in this shortcut slot", keys: [] },
    { id: "env_action_5", cmdZh: "环境操作 5", cmdEn: "Environment action 5", descZh: "在此快捷键槽位中运行环境操作", descEn: "Run the environment action in this shortcut slot", keys: [] },
    { id: "open_commit_push", cmdZh: "打开提交或推送选项", cmdEn: "Open commit or push options", descZh: "打开提交或推送选项", descEn: "Open commit or push options", keys: [] },
    { id: "create_pr", cmdZh: "创建拉取请求 (PR)", cmdEn: "Create PR", descZh: "打开拉取请求创建选项", descEn: "Open pull request creation options", keys: [] },
    { id: "open_folder", cmdZh: "打开文件夹", cmdEn: "Open folder", descZh: "向 Yode 添加本地项目", descEn: "Add a local project to Yode", keys: ["⌘O"] },
    { id: "force_reload_skills", cmdZh: "强制重新加载技能", cmdEn: "Force reload skills", descZh: "为当前上下文刷新技能目录", descEn: "Refresh the skill catalog for the current context", keys: [] },
    { id: "go_to_skills", cmdZh: "转到技能", cmdEn: "Go to skills", descZh: "浏览已安装和推荐的技能", descEn: "Browse installed and recommended skills", keys: [] },
    { id: "install_workspace", cmdZh: "安装 Yode 工作区", cmdEn: "Install Yode Workspace", descZh: "安装高级本地功能的依赖项", descEn: "Install dependencies for advanced local features", keys: [] },
    { id: "kbd_shortcuts", cmdZh: "键盘快捷键", cmdEn: "Keyboard shortcuts", descZh: "自定义键盘快捷键", descEn: "Customize keyboard shortcuts", keys: [] },
    { id: "mcp_config", cmdZh: "MCP", cmdEn: "MCP", descZh: "配置 MCP 服务器", descEn: "Configure MCP servers", keys: [] },
    { id: "personality_config", cmdZh: "人设风格", cmdEn: "Personality", descZh: "调整语气与响应风格", descEn: "Adjust tone and response style", keys: [] },
    { id: "feedback", cmdZh: "反馈", cmdEn: "Feedback", descZh: "向 Yode 团队发送 product 反馈", descEn: "Send product feedback to the Yode team", keys: [] },
    { id: "logout", cmdZh: "退出登录", cmdEn: "Log out", descZh: "登出 Yode", descEn: "Sign out of Yode", keys: [] },
    { id: "manage_automations", cmdZh: "管理自动化", cmdEn: "Manage automations", descZh: "从当前上下文创建或管理自动化", descEn: "Create or manage automations from the current context", keys: [] },
    { id: "wake_pet", cmdZh: "唤醒宠物", cmdEn: "Wake Pet", descZh: "打开宠物悬停窗口", descEn: "Open the pet overlay", keys: [] },
    { id: "open_control_window", cmdZh: "打开控制窗口", cmdEn: "Open control window", descZh: "打开语音控制窗口", descEn: "Open the voice control window", keys: [] },
    { id: "settings", cmdZh: "设置", cmdEn: "Settings", descZh: "打开 Yode 设置", descEn: "Open Yode settings", keys: ["⌘,"] },
    { id: "approve_req", cmdZh: "批准请求", cmdEn: "Approve request", descZh: "批准当前请求", descEn: "Approve the active request", keys: ["↩"] },
    { id: "decline_req", cmdZh: "拒绝请求", cmdEn: "Decline request", descZh: "拒绝当前请求", descEn: "Decline the active request", keys: ["Escape"] },
    { id: "close_tab", cmdZh: "关闭", cmdEn: "Close", descZh: "关闭当前标签页或窗口", descEn: "Close the active tab or window", keys: ["⌘W"] },
    { id: "cycle_reasoning", cmdZh: "循环切换推理强度", cmdEn: "Cycle reasoning effort", descZh: "在输入框中循环切换推理强度", descEn: "Cycle through composer reasoning effort levels", keys: [] },
    { id: "decrease_reasoning", cmdZh: "降低推理强度", cmdEn: "Decrease reasoning effort", descZh: "降低当前输入框推理强度", descEn: "Decrease the current composer reasoning effort level", keys: [] },
    { id: "increase_reasoning", cmdZh: "提高推理强度", cmdEn: "Increase reasoning effort", descZh: "提高当前输入框推理强度", descEn: "Increase the current composer reasoning effort level", keys: [] },
    { id: "open_model_picker", cmdZh: "打开模型选择器", cmdEn: "Open model picker", descZh: "打开输入框模型选择器", descEn: "Open the composer model picker", keys: ["⌃⇧M"] },
    { id: "start_dictation", cmdZh: "启动听写", cmdEn: "Start dictation", descZh: "在当前输入框中启动听写", descEn: "Start dictation in the current composer", keys: ["⌃⇧D"] },
    { id: "toggle_voice", cmdZh: "切换语音模式", cmdEn: "Toggle voice mode", descZh: "启动或停止语音模式", descEn: "Start or stop voice mode", keys: ["⌃⇧V"] },
    { id: "send_msg", cmdZh: "发送消息", cmdEn: "Send message", descZh: "发送当前输入框中的消息", descEn: "Send the current composer message", keys: [] },
    { id: "toggle_fast", cmdZh: "切换快速模式", cmdEn: "Toggle Fast mode", descZh: "在当前输入框中开启或关闭快速模式", descEn: "Turn Fast mode on or off in the current composer", keys: [] },
    { id: "toggle_plan", cmdZh: "切换计划模式", cmdEn: "Toggle plan mode", descZh: "在当前输入框中开启或关闭计划模式", descEn: "Turn plan mode on or off in the current composer", keys: [] },
    { id: "copy_markdown", cmdZh: "复制为 Markdown", cmdEn: "Copy as Markdown", descZh: "将当前对话复制为 Markdown", descEn: "Copy the current chat as Markdown", keys: [] },
    { id: "copy_conv_path", cmdZh: "复制对话路径", cmdEn: "Copy conversation path", descZh: "复制当前对话路径", descEn: "Copy the current chat path", keys: ["⌥⇧⌘C"] },
    { id: "copy_deeplink", cmdZh: "复制深层链接", cmdEn: "Copy deeplink", descZh: "复制当前对话的深层链接", descEn: "Copy a deeplink to the current chat", keys: ["⌥⌘L"] },
    { id: "copy_session_id", cmdZh: "复制会话 ID", cmdEn: "Copy session id", descZh: "复制当前对话会话 ID", descEn: "Copy the current chat session ID", keys: ["⌥⌘C"] },
    { id: "copy_work_dir", cmdZh: "复制工作目录", cmdEn: "Copy working directory", descZh: "复制当前对话的工作目录", descEn: "Copy the current chat working directory", keys: ["⇧⌘C"] },
    { id: "fork_chat", cmdZh: "复刻对话", cmdEn: "Fork chat", descZh: "复刻当前对话", descEn: "Fork the current chat", keys: [] },
    { id: "rename_chat", cmdZh: "重命名对话", cmdEn: "Rename chat", descZh: "重命名当前对话", descEn: "Rename the current chat", keys: ["⌥⌘R"] },
    { id: "search_chats", cmdZh: "搜索对话", cmdEn: "Search Chats...", descZh: "搜索对话记录", descEn: "Search chats", keys: ["⌘G"] },
    { id: "search_files", cmdZh: "搜索文件", cmdEn: "Search Files...", descZh: "搜索工作区中的文件", descEn: "Search files", keys: ["⌘P"] },
    { id: "show_kbd_shortcuts", cmdZh: "显示键盘快捷键", cmdEn: "Show keyboard shortcuts", descZh: "立即显示可用快捷键", descEn: "Show the shortcuts available right now", keys: ["⌘?"] },
    { id: "go_to_chat_1", cmdZh: "转到对话 1", cmdEn: "Go to chat 1", descZh: "在此快捷键槽位中打开可见的对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘1"] },
    { id: "go_to_chat_2", cmdZh: "转到对话 2", cmdEn: "Go to chat 2", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘2"] },
    { id: "go_to_chat_3", cmdZh: "转到对话 3", cmdEn: "Go to chat 3", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘3"] },
    { id: "go_to_chat_4", cmdZh: "转到对话 4", cmdEn: "Go to chat 4", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘4"] },
    { id: "go_to_chat_5", cmdZh: "转到对话 5", cmdEn: "Go to chat 5", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘5"] },
    { id: "go_to_chat_6", cmdZh: "转到对话 6", cmdEn: "Go to chat 6", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘6"] },
    { id: "go_to_chat_7", cmdZh: "转到对话 7", cmdEn: "Go to chat 7", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘7"] },
    { id: "go_to_chat_8", cmdZh: "转到对话 8", cmdEn: "Go to chat 8", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘8"] },
    { id: "go_to_chat_9", cmdZh: "转到对话 9", cmdEn: "Go to chat 9", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘9"] },
    { id: "toggle_file_tree", cmdZh: "切换文件树", cmdEn: "Toggle File Tree", descZh: "切换文件树面板的显示与隐藏", descEn: "Toggle the file tree panel", keys: ["⇧⌘E"] },
    { id: "toggle_max_side_panel", cmdZh: "最大化/还原侧栏面板", cmdEn: "Toggle maximize side panel", descZh: "展开或还原侧栏面板", descEn: "Expand or restore the side panel", keys: [] },
    { id: "start_trace_rec", cmdZh: "开始/停止追踪录制", cmdEn: "Start Trace Recording", descZh: "启动或停止追踪录制", descEn: "Start or stop trace recording", keys: ["⇧⌘S"] }
  ]);

  const handleDeleteBinding = (id: string, keyIdx: number) => {
    setBindings((prev) =>
      prev.map((b) => {
        if (b.id === id) {
          const nextKeys = [...b.keys];
          nextKeys.splice(keyIdx, 1);
          return { ...b, keys: nextKeys };
        }
        return b;
      })
    );
  };

  const filteredBindings = bindings.filter(
    (b) =>
      b.cmdZh.toLowerCase().includes(searchQuery.toLowerCase()) ||
      b.cmdEn.toLowerCase().includes(searchQuery.toLowerCase()) ||
      b.descZh.toLowerCase().includes(searchQuery.toLowerCase()) ||
      b.descEn.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
      <div style={{ position: "relative", width: "100%" }}>
        <Search size={13} style={{ position: "absolute", left: "10px", top: "8px", color: "var(--text-soft)", opacity: 0.8 }} />
        <input
          type="text"
          placeholder={t("搜索快捷键...", "Search shortcuts...")}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{
            width: "100%",
            height: "28px",
            background: "var(--field)",
            border: "1px solid var(--line-soft)",
            borderRadius: "var(--radius)",
            paddingLeft: "28px",
            paddingRight: "28px",
            fontSize: "12px",
            color: "var(--text)",
            outline: "none"
          }}
        />
        <SlidersHorizontal size={13} style={{ position: "absolute", right: "10px", top: "8px", color: "var(--text-soft)", opacity: 0.8 }} />
      </div>

      <div className="theme-card" style={{ padding: "0 12px 12px" }}>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 200px",
            paddingBlock: "10px",
            borderBottom: "1px solid var(--line-soft)",
            fontSize: "11px",
            fontWeight: "700",
            color: "var(--text-soft)",
            textTransform: "uppercase",
            letterSpacing: "0.5px"
          }}
        >
          <span>{t("命令", "Command")}</span>
          <span>{t("快捷键", "Keybinding")}</span>
        </div>

        <div style={{ display: "flex", flexDirection: "column" }}>
          {filteredBindings.map((item) => (
            <div
              key={item.id}
              style={{
                display: "grid",
                gridTemplateColumns: "1fr 200px",
                paddingBlock: "12px",
                borderBottom: "1px solid var(--line-soft)",
                fontSize: "12px"
              }}
            >
              <div style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
                <span style={{ fontWeight: "600", color: "var(--text)" }}>{t(item.cmdZh, item.cmdEn)}</span>
                <span style={{ fontSize: "11px", color: "var(--text-soft)" }}>{t(item.descZh, item.descEn)}</span>
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "6px", justifyContent: "center" }}>
                {item.keys.length === 0 ? (
                  <span style={{ fontSize: "11px", color: "var(--text-soft)", opacity: 0.6 }}>Unassigned</span>
                ) : (
                  item.keys.map((k, idx) => (
                    <div
                      key={k}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "space-between",
                        background: "var(--field)",
                        border: "1px solid var(--line-soft)",
                        borderRadius: "var(--radius)",
                        paddingInline: "8px",
                        paddingBlock: "2px",
                        fontSize: "11px",
                        color: "var(--text)",
                        fontFamily: "var(--font-code)",
                        width: "100%",
                        maxWidth: "160px"
                      }}
                    >
                      <span>{k}</span>
                      <button
                        onClick={() => handleDeleteBinding(item.id, idx)}
                        type="button"
                        style={{
                          background: "transparent",
                          border: "none",
                          cursor: "pointer",
                          color: "var(--text-soft)",
                          padding: "1px 2px",
                          display: "flex",
                          alignItems: "center"
                        }}
                        onMouseEnter={(e) => (e.currentTarget.style.color = "oklch(67% 0.15 28)")}
                        onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-soft)")}
                      >
                        <X size={12} />
                      </button>
                    </div>
                  ))
                )}
              </div>
            </div>
          ))}
          {filteredBindings.length === 0 && (
            <div style={{ paddingBlock: "24px", textAlign: "center", color: "var(--text-soft)", fontSize: "12px" }}>
              {t("未找到匹配的快捷键命令", "No matching shortcut commands found")}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
