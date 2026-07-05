import type { UserQuery } from "./askUser";

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
      metadata?: unknown;
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
      kind: "error";
      title: string;
      body: string;
      createdAt?: number;
      metadata?: unknown;
    }
  | {
      id: string;
      kind: "activity_group";
      label: string;
      type: "explore" | "search" | "run" | "mixed" | "other";
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
      metadata?: unknown;
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
      items?: TimelineItem[];
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
  metadata?: unknown;
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

export type PendingUserQuestion = {
  sessionId: string;
  turnId: string;
  title?: string;
  question: string;
  query?: UserQuery;
};

export const fallbackBootstrap: Bootstrap = {
  appVersion: "",
  workspacePath: "",
  provider: "",
  model: "",
  permissionMode: "default",
  sessions: []
};
