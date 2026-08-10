import { existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { defineConfig } from "playwright/test";

/**
 * 桌面前端端到端测试：在真实 Chromium 中通过 mock Tauri 桥验证
 * 断线恢复 / 重放 / 取消 / 会话隔离 / 分页。
 *
 * 前置条件：先执行 `pnpm build` 生成 dist，再由 vite preview 提供服务。
 * 浏览器：优先使用 `playwright install chromium` 下载的 headless shell；
 * 缺失时回退到本机已有的 Chrome for Testing（离线环境兼容）。
 */
function existingBrowserExecutable(): string | undefined {
  const cache = path.join(os.homedir(), "Library", "Caches", "ms-playwright");
  const candidates = [
    path.join(cache, "chromium_headless_shell-1223", "chrome-headless-shell-mac-arm64", "chrome-headless-shell"),
    path.join(
      cache,
      "chromium-1234",
      "chrome-mac-arm64",
      "Google Chrome for Testing.app",
      "Contents",
      "MacOS",
      "Google Chrome for Testing"
    )
  ];
  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate;
  }
  return undefined;
}

const executablePath = existingBrowserExecutable();

export default defineConfig({
  testDir: "./e2e",
  timeout: 60_000,
  // Chrome for Testing 启动较重，限制并发避免资源争抢导致 launch 超时
  fullyParallel: false,
  workers: 2,
  reporter: [["list"]],
  use: {
    baseURL: "http://127.0.0.1:4173",
    trace: "retain-on-failure",
    ...(executablePath ? { launchOptions: { executablePath, timeout: 90_000 } } : {})
  },
  webServer: {
    command: "pnpm exec vite preview --host 127.0.0.1 --port 4173 --strictPort",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: true,
    timeout: 60_000
  }
});
