import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

import { TerminalDrawer } from "./TerminalDrawer";

describe("TerminalDrawer tab controls", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders the tab close control as a separate native focusable button", () => {
    vi.stubGlobal("window", {});

    const markup = renderToStaticMarkup(createElement(TerminalDrawer, {
      isOpen: false,
      onClose: vi.fn(),
      workspacePath: "/workspace",
      conversationId: "conversation-1",
      height: 320,
      onResizeStart: vi.fn(),
      onResizeKeyDown: vi.fn(),
      onResizeReset: vi.fn()
    }));
    const selectStart = markup.indexOf('class="terminal-tab-select"');
    const selectEnd = markup.indexOf("</button>", selectStart);
    const closeStart = markup.indexOf('class="terminal-tab-close"');
    const closeEnd = markup.indexOf("</button>", closeStart) + "</button>".length;
    const closeButton = markup.slice(markup.lastIndexOf("<button", closeStart), closeEnd);

    expect(closeButton).toContain('class="terminal-tab-close"');
    expect(closeButton).toContain('type="button"');
    expect(closeButton).toContain('aria-label="关闭终端 bash 1"');
    expect(closeButton).not.toContain('tabindex="-1"');
    expect(closeStart).toBeGreaterThan(selectEnd);
  });
});
