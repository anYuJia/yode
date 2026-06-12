export type Bootstrap = {
  appVersion: string;
  workspacePath: string;
  provider: string;
  model: string;
  permissionMode: string;
  sessions: SessionSummary[];
};

export type DefaultLlm = {
  provider: string;
  model: string;
};

export type SessionSummary = {
  id: string;
  title: string;
  project?: string | null;
  projectRoot?: string | null;
  provider?: string;
  model?: string;
  updatedAt: string;
  active?: boolean;
};

export type TimelineItem =
  | {
      id: string;
      kind: "user" | "assistant" | "reasoning";
      title: string;
      body: string;
      attachments?: ImageAttachment[];
      meta?: string;
      createdAt?: number;
      reasoningStartedAt?: number;
    }
  | {
      id: string;
      kind: "tool";
      title: string;
      body: string;
      status: "running" | "success" | "blocked";
      tool: string;
      callId?: string;
      createdAt?: number;
      meta?: string;
      result?: string;
      metadata?: any;
    }

  | {
      id: string;
      kind: "permission";
      title: string;
      body: string;
      tool: string;
      risk: string;
      sessionId?: string;
      turnId?: string;
      createdAt?: number;
    }
  | {
      id: string;
      kind: "boundary";
      title: string;
      body: string;
      createdAt?: number;
    }
  | {
      id: string;
      kind: "process_note";
      title?: string;
      body: string;
      status: "running" | "success";
      createdAt?: number;
    }
  | {
      id: string;
      kind: "activity_group";
      label: string;
      type: "explore" | "search" | "run" | "other";
      status: "running" | "success";
      items: TimelineItem[];
      createdAt?: number;
    }
  | {
      id: string;
      kind: "activity_item";
      type: "edit";
      tool: string;
      title: string;
      body: string;
      status: "running" | "success" | "blocked";
      callId?: string;
      filename?: string;
      diff?: string;
      result?: string;
      metadata?: any;
      createdAt?: number;
    }
  | {
      id: string;
      kind: "edit_summary";
      status: "running" | "success" | "blocked";
      items: Array<Extract<TimelineItem, { kind: "activity_item" }>>;
      createdAt?: number;
    }
  | {
      id: string;
      kind: "tool_group";
      label: string;
      icon: string;
      type: "explore" | "search" | "edit" | "run" | "other";
      status: "running" | "success";
      items?: any[];
      createdAt?: number;
    };

export type DesktopEvent = {
  sessionId: string;
  turnId: string;
  seq: number;
  kind: string;
  timestamp: string;
  payload: Record<string, unknown>;
};

export type DesktopMessage = {
  id: number;
  role: string;
  content?: string | null;
  reasoning?: string | null;
  toolCallsJson?: string | null;
  toolCallId?: string | null;
  images?: Array<{
    base64: string;
    mediaType: string;
  }>;
  createdAt: string;
};

export type TurnAccepted = {
  sessionId: string;
  turnId: string;
  session: SessionSummary;
};

export type ImageAttachment = {
  id: string;
  name: string;
  mediaType: string;
  base64: string;
  dataUrl: string;
  size: number;
};

export const fallbackBootstrap: Bootstrap = {
  appVersion: "0.0.19",
  workspacePath: "/Users/pyu/code/yode",
  provider: "anthropic",
  model: "claude-sonnet-4",
  permissionMode: "Default",
  sessions: []
};

export const sessions: SessionSummary[] = [
  {
    id: "s-1",
    title: "桌面端迁移计划",
    project: "yode",
    projectRoot: "/Users/pyu/code/yode",
    updatedAt: "刚刚",
    active: true
  },
  {
    id: "s-2",
    title: "权限治理审查",
    project: "yode",
    projectRoot: "/Users/pyu/code/yode",
    updatedAt: "14:18"
  },
  {
    id: "s-3",
    title: "AgentEngine event bridge",
    project: "yode",
    projectRoot: "/Users/pyu/code/yode",
    updatedAt: "昨天"
  },
  {
    id: "s-4",
    title: "临时排查记录",
    project: null,
    updatedAt: "05月19日"
  }
];

export const timeline: TimelineItem[] = [
  {
    id: "t-1",
    kind: "user",
    title: "用户",
    body: "把 Yode 做成桌面端 app，参考 Codex 的信息架构，先完成第一批 scaffold。",
    meta: "工作区 yode"
  },
  {
    id: "t-2",
    kind: "reasoning",
    title: "浏览器预览",
    body: "当前页面运行在普通浏览器环境时，会展示静态预览数据；桌面端会通过 Tauri IPC 连接真实 AgentEngine。",
    meta: "preview"
  },
  {
    id: "t-3",
    kind: "tool",
    title: "读取项目结构",
    body: "发现 Rust workspace、核心 crates、TUI 入口和既有桌面迁移计划。",
    tool: "rg --files",
    status: "success",
    meta: "42 files scanned"
  },
  {
    id: "t-4",
    kind: "permission",
    title: "等待确认",
    body: "bash 将执行 `cargo test -p yode-core`。该动作会运行本地测试，不会写入业务文件。",
    tool: "bash",
    risk: "read / execute"
  },
  {
    id: "t-5",
    kind: "tool",
    title: "渲染预览事件",
    body: "sidebar、topbar、timeline、composer 和 settings shell 正在使用预览数据渲染。",
    tool: "tauri command",
    status: "running",
    meta: "preview stream"
  },
  {
    id: "t-6",
    kind: "boundary",
    title: "上下文已压缩",
    body: "保留当前计划、文件变更摘要和下一批 runtime bridge 边界。"
  },
  {
    id: "t-7",
    kind: "assistant",
    title: "Yode",
    body: "这是非桌面环境的预览内容。打开 Tauri 桌面端后，输入、输出、工具事件和历史消息会走真实后端链路。",
    meta: "stream complete"
  }
];
