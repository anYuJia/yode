// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createRoot } from "react-dom/client";
import React, { act } from "react";

import { invoke } from "@tauri-apps/api/core";
import { PermissionActions } from "./PermissionActions";
import type { TimelineItem } from "../../lib/desktopTypes";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn()
}));

const invokeMock = vi.mocked(invoke);
const originalActEnvironment = (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;

function permissionItem(id: string, turnId = `turn-${id}`): Extract<TimelineItem, { kind: "permission" }> {
  return {
    id,
    kind: "permission",
    title: "需要确认",
    body: "cargo test",
    tool: "bash",
    risk: "high",
    sessionId: "session-1",
    turnId
  };
}

function renderActions(item: Extract<TimelineItem, { kind: "permission" }>, onResolved = vi.fn()) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);
  const render = (nextItem = item) => {
    act(() => {
      root.render(
        <PermissionActions item={nextItem} appLang="zh" onResolved={onResolved} />
      );
    });
  };
  render();
  return {
    container,
    root,
    render,
    onResolved,
    unmount: () => {
      act(() => root.unmount());
      container.remove();
    }
  };
}

function submitButton(container: HTMLElement) {
  const button = container.querySelector<HTMLButtonElement>("button.permission-submit");
  if (!button) throw new Error("permission submit button not found");
  return button;
}

beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = "";
  if (originalActEnvironment === undefined) {
    delete (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
  } else {
    (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = originalActEnvironment;
  }
});

describe("PermissionActions submission guard", () => {
  it("submits only once when click and keyboard-equivalent actions race", async () => {
    let resolveRequest: (() => void) | undefined;
    const request = new Promise<void>((resolve) => {
      resolveRequest = resolve;
    });
    invokeMock.mockReturnValue(request);
    const view = renderActions(permissionItem("one"));

    act(() => {
      submitButton(view.container).click();
      submitButton(view.container).click();
    });

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(submitButton(view.container).disabled).toBe(true);

    await act(async () => {
      resolveRequest?.();
      await request;
    });

    expect(view.onResolved).toHaveBeenCalledTimes(1);
    view.unmount();
  });

  it("resets the submission state when the active permission request changes", async () => {
    invokeMock.mockResolvedValue(undefined);
    const view = renderActions(permissionItem("first"));

    act(() => submitButton(view.container).click());
    await act(async () => {
      await Promise.resolve();
    });
    expect(submitButton(view.container).disabled).toBe(true);

    view.render(permissionItem("second"));
    expect(submitButton(view.container).disabled).toBe(false);

    act(() => submitButton(view.container).click());
    await act(async () => {
      await Promise.resolve();
    });
    expect(invokeMock).toHaveBeenCalledTimes(2);
    view.unmount();
  });
});
