import { describe, expect, it, vi } from "vitest";

import {
  cycleDialogFocus,
  focusFirstDialogControl,
  restoreDialogTriggerFocus
} from "./GeneralSettings";

function focusableControl() {
  return { focus: vi.fn() } as unknown as HTMLElement;
}

function dialogWithControls(controls: HTMLElement[]) {
  return {
    focus: vi.fn(),
    querySelectorAll: vi.fn(() => controls)
  } as unknown as HTMLElement;
}

describe("许可声明弹窗焦点管理", () => {
  it("打开时将焦点放在首个对话框控件，关闭时回到触发控件", () => {
    const closeButton = focusableControl();
    const triggerButton = focusableControl();
    const dialog = dialogWithControls([closeButton]);

    focusFirstDialogControl(dialog, null);
    restoreDialogTriggerFocus(triggerButton);

    expect(closeButton.focus).toHaveBeenCalledOnce();
    expect(triggerButton.focus).toHaveBeenCalledOnce();
  });

  it("在对话框内循环 Tab 与 Shift+Tab 焦点", () => {
    const first = focusableControl();
    const second = focusableControl();
    const dialog = dialogWithControls([first, second]);

    cycleDialogFocus(dialog, second as unknown as Element, false);
    cycleDialogFocus(dialog, first as unknown as Element, true);

    expect(first.focus).toHaveBeenCalledOnce();
    expect(second.focus).toHaveBeenCalledOnce();
  });
});
