export type Bootstrap = {
  appVersion: string;
  workspacePath: string;
  provider: string;
  model: string;
  permissionMode: string;
  sessions: SessionSummary[];
};

export type SessionSummary = {
  id: string;
  title: string;
  project: string;
  updatedAt: string;
  active?: boolean;
};

export type TimelineItem =
  | {
      id: string;
      kind: "user" | "assistant" | "reasoning";
      title: string;
      body: string;
      meta?: string;
    }
  | {
      id: string;
      kind: "tool";
      title: string;
      body: string;
      status: "running" | "success" | "blocked";
      tool: string;
      meta?: string;
    }
  | {
      id: string;
      kind: "permission";
      title: string;
      body: string;
      tool: string;
      risk: string;
    }
  | {
      id: string;
      kind: "boundary";
      title: string;
      body: string;
    };

export type DesktopEvent = {
  sessionId: string;
  turnId: string;
  seq: number;
  kind: string;
  timestamp: string;
  payload: Record<string, unknown>;
};

export type TurnAccepted = {
  sessionId: string;
  turnId: string;
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
    updatedAt: "刚刚",
    active: true
  },
  {
    id: "s-2",
    title: "权限治理审查",
    project: "yode",
    updatedAt: "14:18"
  },
  {
    id: "s-3",
    title: "AgentEngine event bridge",
    project: "yode",
    updatedAt: "昨天"
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
    title: "运行时分析",
    body: "当前批次只建立桌面 shell 和 mock event log。真实 AgentEngine bridge 留到下一批，避免 UI 与 runtime 问题混在一起。",
    meta: "plan"
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
    title: "生成 UI shell",
    body: "sidebar、topbar、timeline、composer 和 settings shell 已准备渲染。",
    tool: "tauri command",
    status: "running",
    meta: "mock stream"
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
    body: "第一批目标是让桌面窗口可以启动，并提供足够接近真实任务的交互骨架。接下来会把 EngineEvent 映射成 DesktopEvent。",
    meta: "stream complete"
  }
];
