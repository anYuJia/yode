import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

import { SettingsFileWarning, SettingsFileWarningProps } from "./SettingsFileWarning";

describe("SettingsFileWarning", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  function render(props: Partial<SettingsFileWarningProps> = {}) {
    const base: SettingsFileWarningProps = {
      status: { loaded: false, path: "/home/user/.yode/desktop-settings.json", error: "设置文件不是有效 JSON，尚未加载" },
      isZh: true,
      onOpenFile: vi.fn(),
      onRetry: vi.fn(),
      onRestore: vi.fn()
    };
    return {
      props: { ...base, ...props },
      markup: renderToStaticMarkup(createElement(SettingsFileWarning, { ...base, ...props }))
    };
  }

  it("renders the not-loaded banner with Chinese copy and accessible actions", () => {
    const { markup } = render();

    expect(markup).toContain('role="alert"');
    expect(markup).toContain("设置文件未加载");
    expect(markup).toContain("无法读取或解析桌面设置文件");
    expect(markup).toContain("打开配置位置");
    expect(markup).toContain("重试读取");
    expect(markup).toContain("恢复默认设置");
    expect(markup).toContain("设置文件不是有效 JSON，尚未加载");
    // 三个操作都是原生按钮
    expect(markup.match(/<button/g)).toHaveLength(3);
    expect(markup.match(/type="button"/g)).toHaveLength(3);
  });

  it("renders English copy when the app language is not zh", () => {
    const { markup } = render({ isZh: false });
    expect(markup).toContain("Settings file not loaded");
    expect(markup).toContain("Retry");
    expect(markup).toContain("Restore defaults");
    expect(markup).not.toContain("设置文件未加载");
  });

  it("renders nothing when the settings file is loaded", () => {
    const { markup } = render({ status: { loaded: true, path: "/x" } });
    expect(markup).toBe("");
  });

  it("renders a generic message when no error detail is available", () => {
    const { markup } = render({ status: { loaded: false, path: "/x" } });
    expect(markup).toContain("设置文件未加载");
  });
});
