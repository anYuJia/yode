import { useEffect } from "react";

import { runsList } from "./desktopIpc";
import { useAppUiStore } from "./appUiStore";

/**
 * 周期性刷新 runs（数据库 turn journal 的桌面投影）。
 *
 * 事件驱动为主（实时事件即时更新），本轮询作为兜底：
 * - 恢复 RunInspector 的 currentRun 状态（等待/取消中/终态诊断）；
 * - 修复事件通道短暂断开后 run 状态滞后的问题；
 * - 标签页隐藏时不轮询，回到前台立即刷新一次；
 * - 请求在途时跳过本次（不叠加并发查询）。
 */
export function useRunsPoller(intervalMs = 30_000) {
  const setRuns = useAppUiStore((state) => state.setRuns);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) {
      return;
    }
    let timer: number | undefined;
    let inFlight = false;

    const poll = async () => {
      if (inFlight || document.visibilityState !== "visible") return;
      inFlight = true;
      try {
        const runs = await runsList();
        setRuns(runs);
      } catch (err) {
        // 轮询失败不影响运行态：下一次间隔自动重试
        console.warn("[runs-poller] 刷新运行状态失败:", err);
      } finally {
        inFlight = false;
      }
    };

    const onVisibility = () => {
      if (document.visibilityState === "visible") {
        void poll();
      }
    };
    document.addEventListener("visibilitychange", onVisibility);
    timer = window.setInterval(() => void poll(), intervalMs);
    return () => {
      if (timer !== undefined) {
        window.clearInterval(timer);
      }
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [setRuns, intervalMs]);
}
