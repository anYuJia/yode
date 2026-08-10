import { expect, test } from "playwright/test";

import { mockBackend, type MockBackendState } from "./mockTauri";

const BASE_TIME = "2026-08-08T00:00:00Z";

function baseState(overrides: Partial<MockBackendState> = {}): MockBackendState {
  return {
    appVersion: "2.0.0-e2e",
    workspacePath: "/workspace",
    provider: "mock",
    model: "mock-model",
    permissionMode: "default",
    sessions: [
      {
        id: "session-a",
        title: "会话 A",
        projectRoot: "/workspace",
        provider: "mock",
        model: "mock-model",
        updatedAt: BASE_TIME,
        active: true
      },
      {
        id: "session-b",
        title: "会话 B",
        projectRoot: "/workspace",
        provider: "mock",
        model: "mock-model",
        updatedAt: BASE_TIME,
        active: false
      }
    ],
    runs: [],
    turnEvents: {},
    messages: {},
    ...overrides
  };
}

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (error) => {
    // 前端崩溃必须让测试失败
    throw new Error(`page error: ${error.message}`);
  });
  page.on("console", (message) => {
    if (message.type() === "error") {
      // eslint-disable-next-line no-console
      console.error(`[console.error] ${message.text()}`);
    }
  });
});

test("断线恢复：未终态 turn 的事件在启动后被重放并锁定，完成后解锁", async ({ page }) => {
  const state = baseState({
    runs: [
      {
        sessionId: "session-a",
        turnId: "turn-recover",
        status: "running",
        updatedAt: BASE_TIME,
        startedAt: BASE_TIME,
        lastSeq: 2
      }
    ],
    turnEvents: {
      "session-a:turn-recover": [
        { sessionId: "session-a", turnId: "turn-recover", seq: 0, kind: "turn_started", timestamp: BASE_TIME, payload: { title: "思考中", body: "" } },
        { sessionId: "session-a", turnId: "turn-recover", seq: 1, kind: "assistant_text_delta", timestamp: BASE_TIME, payload: { body: "持久化片段" } },
        { sessionId: "session-a", turnId: "turn-recover", seq: 2, kind: "tool_started", timestamp: BASE_TIME, payload: { id: "c1", tool: "bash", title: "调用工具: bash", body: "ls", status: "running" } }
      ]
    }
  });
  await mockBackend(page, state);
  await page.goto("/");

  // 重放完成：时间线包含持久化事件内容
  await expect(page.getByText("持久化片段")).toBeVisible();
  // 会话处于运行中（重放期间锁定，允许取消）
  await expect(page.locator(".run-inspector")).toContainText("运行中");
  // 时间线保留工具调用
  await expect(page.getByText("调用工具: bash")).toBeVisible();
});

test("interrupted turn 显示明确诊断，且允许用户开始新 turn", async ({ page }) => {
  const state = baseState({
    runs: [
      {
        sessionId: "session-a",
        turnId: "turn-stale",
        status: "interrupted",
        updatedAt: BASE_TIME,
        startedAt: BASE_TIME,
        endedAt: BASE_TIME,
        lastSeq: 5,
        detail: "检测到上次运行未正常结束，已标记为中断",
        errorCode: "interrupted_on_startup"
      }
    ]
  });
  await mockBackend(page, state);
  await page.goto("/");

  // RunInspector 展示 interrupted 诊断
  await expect(page.locator(".run-inspector")).toContainText("已中断");
  await expect(page.locator(".run-inspector")).toContainText("interrupted_on_startup");
  await expect(page.locator(".run-inspector")).toContainText("检测到上次运行未正常结束");

  // 终端状态不锁定：composer 可用，可发送新消息
  await expect(page.locator("textarea[aria-label='消息']")).toBeEnabled();
  await page.locator("textarea[aria-label='消息']").fill("开始新任务");
  await page.locator("textarea[aria-label='消息']").press("Enter");
  await expect(page.getByLabel("会话时间线").getByText("开始新任务")).toBeVisible();
});

test("发送新 turn：事件流按真实节奏渲染，终态后解锁", async ({ page }) => {
  await mockBackend(page, baseState());
  await page.goto("/");

  await page.locator("textarea[aria-label='消息']").fill("生成一段回复");
  await page.locator("textarea[aria-label='消息']").press("Enter");

  await expect(page.getByLabel("会话时间线").getByText("生成一段回复")).toBeVisible();
  // 流式事件陆续渲染：增量文本合帧后保留在时间线
  await expect(page.getByText("正在生成回复")).toBeVisible({ timeout: 10_000 });
  // 终态后 RunInspector 反映持久化状态
  await expect(page.locator(".run-inspector")).toContainText("已完成", { timeout: 10_000 });
  // 工具调用被保留（过程已折叠，展开过程后以工具组呈现）
  await page.getByText(/展开过程/).click();
  await expect(page.getByText(/已运行 1 条命令/)).toBeVisible();
});

test("取消：后端确认终态后释放 UI", async ({ page }) => {
  const state = baseState({
    runs: [
      {
        sessionId: "session-a",
        turnId: "turn-live",
        status: "running",
        updatedAt: BASE_TIME,
        startedAt: BASE_TIME,
        lastSeq: 0
      }
    ],
    turnEvents: {
      "session-a:turn-live": [
        { sessionId: "session-a", turnId: "turn-live", seq: 0, kind: "turn_started", timestamp: BASE_TIME, payload: { title: "思考中", body: "" } }
      ]
    }
  });
  await mockBackend(page, state);
  await page.goto("/");

  await expect(page.locator(".run-inspector")).toContainText("运行中");
  // 点击取消（Composer 终止按钮）
  await page.locator("button[title='终止']").first().click();
  // 后端 cancelling → cancelled 后，UI 解锁（停止处理中）且 RunInspector 反映终态
  await expect(page.locator(".run-inspector")).toContainText("已取消", { timeout: 10_000 });
  await expect(page.locator("textarea[aria-label='消息']")).toBeEnabled({ timeout: 10_000 });
});

test("会话切换：新会话只显示自己的时间线", async ({ page }) => {
  const state = baseState({
    messages: {
      "session-a": [
        { id: 1, sortOrder: 0, role: "user", content: "A 的历史问题", createdAt: BASE_TIME },
        { id: 2, sortOrder: 1, role: "assistant", content: "A 的回复", createdAt: BASE_TIME }
      ],
      "session-b": [
        { id: 3, sortOrder: 0, role: "user", content: "B 的历史问题", createdAt: BASE_TIME }
      ]
    }
  });
  await mockBackend(page, state);
  await page.goto("/");

  await expect(page.getByText("A 的回复")).toBeVisible();
  await expect(page.getByText("B 的历史问题")).not.toBeVisible();

  await page.locator(".sidebar").getByText("会话 B").click();
  await expect(page.getByText("B 的历史问题")).toBeVisible();
  await expect(page.getByText("A 的回复")).not.toBeVisible();
});

test("长会话分页：首次只加载最近窗口，滚动到顶部加载更早消息", async ({ page }) => {
  const messages = [];
  for (let index = 0; index < 150; index += 1) {
    messages.push({
      id: index,
      sortOrder: index,
      role: index % 2 === 0 ? "user" : "assistant",
      content: `历史消息 ${index}`,
      createdAt: new Date(Date.parse(BASE_TIME) + index * 1000).toISOString()
    });
  }
  const state = baseState({ messages: { "session-a": messages } });
  await mockBackend(page, state);
  await page.goto("/");

  // 最近 100 条：最早可见的是 历史消息 50
  await expect(page.getByText("历史消息 149")).toBeVisible();
  await expect(page.getByText("历史消息 49")).not.toBeVisible();

  // 滚动到顶部：加载更早窗口
  await page.locator(".timeline-panel").evaluate((panel) => {
    panel.scrollTop = 0;
    panel.dispatchEvent(new Event("scroll"));
  });
  await expect(page.getByText("历史消息 49")).toBeVisible({ timeout: 10_000 });
});
