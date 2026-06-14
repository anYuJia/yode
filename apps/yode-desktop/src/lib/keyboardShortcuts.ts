export type ShortcutBinding = {
  id: string;
  cmdZh: string;
  cmdEn: string;
  descZh: string;
  descEn: string;
  keys: string[];
};

export const KEYBOARD_SHORTCUTS_STORAGE_KEY = "yode-keyboard-shortcuts";

export const DEFAULT_SHORTCUT_BINDINGS: ShortcutBinding[] = [
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
  { id: "open_folder", cmdZh: "打开文件夹", cmdEn: "Open folder", descZh: "向 Yode 添加本地项目", descEn: "Add a local project to Yode", keys: ["⌘O"] },
  { id: "settings", cmdZh: "设置", cmdEn: "Settings", descZh: "打开 Yode 设置", descEn: "Open Yode settings", keys: ["⌘,"] },
  { id: "approve_req", cmdZh: "批准请求", cmdEn: "Approve request", descZh: "批准当前请求", descEn: "Approve the active request", keys: ["↩"] },
  { id: "decline_req", cmdZh: "拒绝请求", cmdEn: "Decline request", descZh: "拒绝当前请求", descEn: "Decline the active request", keys: ["Escape"] },
  { id: "close_tab", cmdZh: "关闭", cmdEn: "Close", descZh: "关闭当前标签页或窗口", descEn: "Close the active tab or window", keys: ["⌘W"] },
  { id: "open_model_picker", cmdZh: "打开模型选择器", cmdEn: "Open model picker", descZh: "打开输入框模型选择器", descEn: "Open the composer model picker", keys: ["⌃⇧M"] },
  { id: "start_dictation", cmdZh: "启动听写", cmdEn: "Start dictation", descZh: "在当前输入框中启动听写", descEn: "Start dictation in the current composer", keys: ["⌃⇧D"] },
  { id: "toggle_voice", cmdZh: "切换语音模式", cmdEn: "Toggle voice mode", descZh: "启动或停止语音模式", descEn: "Start or stop voice mode", keys: ["⌃⇧V"] },
  { id: "send_msg", cmdZh: "发送消息", cmdEn: "Send message", descZh: "发送当前输入框中的消息", descEn: "Send the current composer message", keys: [] },
  { id: "copy_markdown", cmdZh: "复制为 Markdown", cmdEn: "Copy as Markdown", descZh: "将当前对话复制为 Markdown", descEn: "Copy the current chat as Markdown", keys: [] },
  { id: "copy_conv_path", cmdZh: "复制对话路径", cmdEn: "Copy conversation path", descZh: "复制当前对话路径", descEn: "Copy the current chat path", keys: ["⌥⇧⌘C"] },
  { id: "copy_deeplink", cmdZh: "复制深层链接", cmdEn: "Copy deeplink", descZh: "复制当前对话的深层链接", descEn: "Copy a deeplink to the current chat", keys: ["⌥⌘L"] },
  { id: "copy_session_id", cmdZh: "复制会话 ID", cmdEn: "Copy session id", descZh: "复制当前对话会话 ID", descEn: "Copy the current chat session ID", keys: ["⌥⌘C"] },
  { id: "copy_work_dir", cmdZh: "复制工作目录", cmdEn: "Copy working directory", descZh: "复制当前对话的工作目录", descEn: "Copy the current chat working directory", keys: ["⇧⌘C"] },
  { id: "rename_chat", cmdZh: "重命名对话", cmdEn: "Rename chat", descZh: "重命名当前对话", descEn: "Rename the current chat", keys: ["⌥⌘R"] },
  { id: "search_chats", cmdZh: "搜索对话", cmdEn: "Search Chats...", descZh: "搜索对话记录", descEn: "Search chats", keys: ["⌘G"] },
  { id: "search_files", cmdZh: "搜索文件", cmdEn: "Search Files...", descZh: "搜索工作区中的文件", descEn: "Search files", keys: ["⌘P"] },
  { id: "show_kbd_shortcuts", cmdZh: "显示键盘快捷键", cmdEn: "Show keyboard shortcuts", descZh: "立即显示可用快捷键", descEn: "Show the shortcuts available right now", keys: ["⌘?"] },
  { id: "go_to_chat_1", cmdZh: "转到对话 1", cmdEn: "Go to chat 1", descZh: "打开可见列表中的第 1 个对话", descEn: "Open the first visible chat", keys: ["⌘1"] },
  { id: "go_to_chat_2", cmdZh: "转到对话 2", cmdEn: "Go to chat 2", descZh: "打开可见列表中的第 2 个对话", descEn: "Open the second visible chat", keys: ["⌘2"] },
  { id: "go_to_chat_3", cmdZh: "转到对话 3", cmdEn: "Go to chat 3", descZh: "打开可见列表中的第 3 个对话", descEn: "Open the third visible chat", keys: ["⌘3"] },
  { id: "go_to_chat_4", cmdZh: "转到对话 4", cmdEn: "Go to chat 4", descZh: "打开可见列表中的第 4 个对话", descEn: "Open the fourth visible chat", keys: ["⌘4"] },
  { id: "go_to_chat_5", cmdZh: "转到对话 5", cmdEn: "Go to chat 5", descZh: "打开可见列表中的第 5 个对话", descEn: "Open the fifth visible chat", keys: ["⌘5"] },
  { id: "go_to_chat_6", cmdZh: "转到对话 6", cmdEn: "Go to chat 6", descZh: "打开可见列表中的第 6 个对话", descEn: "Open the sixth visible chat", keys: ["⌘6"] },
  { id: "go_to_chat_7", cmdZh: "转到对话 7", cmdEn: "Go to chat 7", descZh: "打开可见列表中的第 7 个对话", descEn: "Open the seventh visible chat", keys: ["⌘7"] },
  { id: "go_to_chat_8", cmdZh: "转到对话 8", cmdEn: "Go to chat 8", descZh: "打开可见列表中的第 8 个对话", descEn: "Open the eighth visible chat", keys: ["⌘8"] },
  { id: "go_to_chat_9", cmdZh: "转到对话 9", cmdEn: "Go to chat 9", descZh: "打开可见列表中的第 9 个对话", descEn: "Open the ninth visible chat", keys: ["⌘9"] },
  { id: "toggle_file_tree", cmdZh: "切换文件树", cmdEn: "Toggle File Tree", descZh: "切换文件树面板的显示与隐藏", descEn: "Toggle the file tree panel", keys: ["⇧⌘E"] },
  { id: "toggle_max_side_panel", cmdZh: "最大化/还原侧栏面板", cmdEn: "Toggle maximize side panel", descZh: "展开或还原侧栏面板", descEn: "Expand or restore the side panel", keys: [] },
  { id: "start_trace_rec", cmdZh: "开始/停止追踪录制", cmdEn: "Start Trace Recording", descZh: "启动或停止追踪录制", descEn: "Start or stop trace recording", keys: ["⇧⌘S"] }
];

export function loadShortcutBindings(): ShortcutBinding[] {
  try {
    const raw = localStorage.getItem(KEYBOARD_SHORTCUTS_STORAGE_KEY);
    if (!raw) return DEFAULT_SHORTCUT_BINDINGS;
    return shortcutBindingsFromOverrides(JSON.parse(raw));
  } catch {
    return DEFAULT_SHORTCUT_BINDINGS;
  }
}

export function shortcutBindingsFromOverrides(overrides: unknown): ShortcutBinding[] {
  if (!Array.isArray(overrides)) return DEFAULT_SHORTCUT_BINDINGS;
  const overrideMap = new Map(
    overrides
      .filter((item): item is { id: string; keys: string[] } =>
        Boolean(item) && typeof item.id === "string" && Array.isArray(item.keys)
      )
      .map((item) => [item.id, item.keys.filter((key): key is string => typeof key === "string")])
  );
  return DEFAULT_SHORTCUT_BINDINGS.map((binding) => ({
    ...binding,
    keys: Array.isArray(overrideMap.get(binding.id)) ? overrideMap.get(binding.id)! : binding.keys
  }));
}

export function saveShortcutBindings(bindings: ShortcutBinding[]) {
  const payload = bindings.map((binding) => ({ id: binding.id, keys: binding.keys }));
  localStorage.setItem(KEYBOARD_SHORTCUTS_STORAGE_KEY, JSON.stringify(payload));
  window.dispatchEvent(new Event("yode-keyboard-shortcuts-change"));
}

export function normalizeShortcutLabel(label: string) {
  return label
    .replace(/\s+/g, "")
    .replace(/CommandOrControl|CmdOrCtrl|Command|Meta|Cmd|⌘/gi, "⌘")
    .replace(/Control|Ctrl|⌃/gi, "⌃")
    .replace(/Option|Alt|⌥/gi, "⌥")
    .replace(/Shift|⇧/gi, "⇧")
    .replace(/Return|Enter|↩/gi, "↩")
    .replace(/Escape|Esc/gi, "Escape")
    .replace(/ArrowRight/gi, "Right")
    .replace(/ArrowLeft/gi, "Left")
    .replace(/ArrowUp/gi, "Up")
    .replace(/ArrowDown/gi, "Down");
}

export function shortcutFromKeyboardEvent(event: KeyboardEvent | React.KeyboardEvent) {
  const key = printableKey(event.key);
  const modifiers = [
    event.ctrlKey ? "⌃" : "",
    event.altKey ? "⌥" : "",
    event.shiftKey ? "⇧" : "",
    event.metaKey ? "⌘" : ""
  ].join("");
  return normalizeShortcutLabel(`${modifiers}${key}`);
}

export function eventMatchesShortcut(event: KeyboardEvent, shortcut: string) {
  return shortcutFromKeyboardEvent(event) === normalizeShortcutLabel(shortcut);
}

export function findShortcutAction(event: KeyboardEvent, bindings = loadShortcutBindings()) {
  return bindings.find((binding) => binding.keys.some((key) => eventMatchesShortcut(event, key)))?.id ?? null;
}

function printableKey(key: string) {
  if (key === " ") return "Space";
  if (key === "Enter") return "↩";
  if (key === "Escape") return "Escape";
  if (key === "ArrowRight") return "Right";
  if (key === "ArrowLeft") return "Left";
  if (key === "ArrowUp") return "Up";
  if (key === "ArrowDown") return "Down";
  if (key === "Backspace") return "Backspace";
  if (key === "Delete") return "Delete";
  if (key === "Tab") return "Tab";
  return key.length === 1 ? key.toUpperCase() : key;
}
