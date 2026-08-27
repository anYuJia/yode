import { useCallback, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

import { appGetBootstrap, configGetProviders, runsList, sessionsMessagesPage } from "./desktopIpc";
import { replayTurnEvents } from "./desktopEventReplay";
import {
  discardPendingDeltas,
  handleDesktopRuntimeEvent,
  isTerminalTurnEvent
} from "./desktopEventHandlers";
import { startCancellationWatchdog } from "./cancellationWatchdog";
import { CancellationWatchdogRegistry } from "./cancellationWatchdogRegistry";
import {
  fallbackBootstrap,
  type Bootstrap,
  type DesktopEvent,
  type DesktopMessage,
  type RunState
} from "./desktopTypes";
import { TERMINAL_RUN_STATUSES } from "./desktopTypes";
import { mergeOlderTimelineItems, messagesToTimelineItems } from "./timelineUtils";
import { dedupeProjectRoots, visibleSessions } from "./projectStorage";
import { saveStoredProviders } from "./llmProviderStorage";
import { useAppUiStore } from "./appUiStore";
import { useRunsPoller } from "./useRunsPoller";

/**
 * 桌面运行时的唯一编排 hook：
 * - 初始化顺序：注册事件监听 → bootstrap → 运行状态 → 事件重放 → 放行新 turn；
 * - 断线恢复与重试（replayState + retryReplay）；
 * - 取消对账 watchdog 的登记簿与工厂（按 sessionId + turnId 双重隔离）；
 * - 会话切换时的 watchdog 挂起/恢复；
 * - 周期性 runs 兜底刷新；
 * - 会话消息分页（最近窗口 + 向上翻页）。
 * App.tsx 不再承载这些业务状态机，只做页面编排。
 */
export function useDesktopRuntimeInit() {
  const setBootstrap = useAppUiStore((state) => state.setBootstrap);
  const setPermissionMode = useAppUiStore((state) => state.setPermissionMode);
  const setSelectedProjectRoot = useAppUiStore((state) => state.setSelectedProjectRoot);
  const setSessionItems = useAppUiStore((state) => state.setSessionItems);
  const setProjectRoots = useAppUiStore((state) => state.setProjectRoots);
  const setActiveSessionId = useAppUiStore((state) => state.setActiveSessionId);
  const activeSessionId = useAppUiStore((state) => state.activeSessionId);
  const getSessionUiState = useAppUiStore((state) => state.getSessionUiState);
  const setCurrentTurnId = useAppUiStore((state) => state.setCurrentTurnId);
  const setIsProcessing = useAppUiStore((state) => state.setIsProcessing);
  const setPendingUserQuestion = useAppUiStore((state) => state.setPendingUserQuestion);
  const setTimelineItems = useAppUiStore((state) => state.setTimelineItems);
  const setUsageSnapshot = useAppUiStore((state) => state.setUsageSnapshot);
  const setRuns = useAppUiStore((state) => state.setRuns);
  const setReplayState = useAppUiStore((state) => state.setReplayState);
  const setHistoryLoading = useAppUiStore((state) => state.setHistoryLoading);
  const setHasMoreHistory = useAppUiStore((state) => state.setHasMoreHistory);
  const setHistoryError = useAppUiStore((state) => state.setHistoryError);
  const setHistoryCursor = useAppUiStore((state) => state.setHistoryCursor);
  const generalSettings = useAppUiStore((state) => state.generalSettings);
  const retryReplayGeneration = useAppUiStore((state) => state.replayState.retryGeneration);

  const activeSessionIdRef = useRef<string | null>(null);
  const currentTurnIdRef = useRef<string | null>(null);
  const windowFocusedRef = useRef(true);
  // 取消轮询登记簿：按 (sessionId, turnId) 幂等维护 watcher 与 pending 登记，
  // 终态事件/新 turn/切会话/卸载时自动清理，杜绝旧 watchdog 解锁新任务。
  const registryRef = useRef(new CancellationWatchdogRegistry());

  // 周期性 runs 兜底刷新：恢复 RunInspector 的持久化状态与终态诊断
  useRunsPoller();

  const updateCancellationBoundary = useCallback(
    (sessionId: string, turnId: string, title: string, body: string) => {
      setTimelineItems((items) => {
        const existing = items.some(
          (item) => item.kind === "boundary" && item.id.includes(`cancel-${turnId}-`)
        );
        if (!existing) {
          return [
            ...items,
            {
              id: `cancel-${turnId}-${Date.now()}`,
              kind: "boundary" as const,
              title,
              body,
              createdAt: Date.now()
            }
          ];
        }
        return items.map((item) =>
          item.kind === "boundary" && item.id.includes(`cancel-${turnId}-`)
            ? { ...item, title, body }
            : item
        );
      }, sessionId);
    },
    [setTimelineItems]
  );

  const createCancellationWatchdog = useCallback(
    (sessionId: string, turnId: string, onConfirmedTerminal: () => void) =>
      startCancellationWatchdog({
        sessionId,
        turnId,
        // 会话 + turn 双重隔离：只有该会话的当前 turn 仍是被取消的 turn 才会继续
        isStillCurrent: () => getSessionUiState(sessionId).currentTurnId === turnId,
        fetchRuns: () => runsList(),
        onReleased: () => {
          // 轮询确认终态：先清理 registry 登记，再解锁 UI
          onConfirmedTerminal();
          let released = false;
          setCurrentTurnId((turnIdNow) => {
            if (turnIdNow !== turnId) return turnIdNow;
            released = true;
            return null;
          }, sessionId);
          if (!released) return; // turn 已切换：不得解锁新任务
          discardPendingDeltas(sessionId);
          setIsProcessing(false, sessionId);
          setPendingUserQuestion(
            (question) => (question?.turnId === turnId ? null : question),
            sessionId
          );
          updateCancellationBoundary(sessionId, turnId, "已结束", "后端已确认本轮运行结束。");
        },
        onWaiting: () => {
          updateCancellationBoundary(sessionId, turnId, "仍在停止", "仍在等待后端确认本轮运行已结束。");
        },
        onQueryError: () => {
          updateCancellationBoundary(sessionId, turnId, "仍在停止", "暂时无法确认停止状态，仍在等待后端终态事件。");
        },
        schedule: (callback, delayMs) => window.setTimeout(callback, delayMs),
        clear: (handle) => window.clearTimeout(handle)
      }),
    [getSessionUiState, setCurrentTurnId, setIsProcessing, setPendingUserQuestion, updateCancellationBoundary]
  );

  /** 发起取消：登记 pending 并武装对账 watchdog（幂等）。 */
  const beginCancellationWatch = useCallback(
    (sessionId: string, turnId: string) => {
      const registry = registryRef.current;
      registry.markPending(sessionId, turnId);
      registry.arm(sessionId, turnId, createCancellationWatchdog).start();
    },
    [createCancellationWatchdog]
  );

  /** 新 turn 接管或删除会话：终止该会话全部 watcher 并清空登记。 */
  const stopSessionWatchdogs = useCallback((sessionId: string) => {
    registryRef.current.stopSession(sessionId);
  }, []);

  /** 终态事件到达：停止对应 turn 的 watchdog 并清理登记。 */
  const stopTurnWatchdog = useCallback((sessionId: string, turnId: string) => {
    registryRef.current.stop(sessionId, turnId);
  }, []);

  // ref 与 store 的 activeSessionId 保持同步（App 手动设置两者，这里兜底）
  useEffect(() => {
    activeSessionIdRef.current = activeSessionId;
  }, [activeSessionId]);

  useEffect(() => {
    const handleFocus = () => {
      windowFocusedRef.current = true;
    };
    const handleBlur = () => {
      windowFocusedRef.current = false;
    };
    const handleVisibility = () => {
      windowFocusedRef.current = document.visibilityState === "visible";
    };
    window.addEventListener("focus", handleFocus);
    window.addEventListener("blur", handleBlur);
    document.addEventListener("visibilitychange", handleVisibility);
    return () => {
      window.removeEventListener("focus", handleFocus);
      window.removeEventListener("blur", handleBlur);
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, []);

  const sendSystemNotification = useCallback(
    (title: string, body: string, policy: "completion" | "permission" | "question") => {
      if (!("Notification" in window)) return;
      if (policy === "permission" && !generalSettings.permissionNotification) return;
      if (policy === "question" && !generalSettings.questionNotification) return;
      if (policy === "completion") {
        if (generalSettings.completionNotification === "Never") return;
        if (generalSettings.completionNotification === "Only when unfocused" && windowFocusedRef.current) return;
      }
      const show = () => new Notification(title, { body });
      if (Notification.permission === "granted") {
        show();
      } else if (Notification.permission === "default") {
        Notification.requestPermission()
          .then((permission) => {
            if (permission === "granted") show();
          })
          .catch(console.error);
      }
    },
    [generalSettings]
  );

  /** 事件分发上下文：实时监听与断线重放共用同一条事件管道。 */
  const buildEventDispatchContext = useCallback(
    () => ({
      activeSessionId: activeSessionIdRef.current,
      currentTurnId: currentTurnIdRef.current,
      getCurrentTurnId: (sessionId: string) => getSessionUiState(sessionId).currentTurnId,
      sendSystemNotification,
      setCurrentTurnId,
      setIsProcessing,
      setPendingUserQuestion,
      setTimelineItems,
      setUsageSnapshot
    }),
    [getSessionUiState, sendSystemNotification, setCurrentTurnId, setIsProcessing, setPendingUserQuestion, setTimelineItems, setUsageSnapshot]
  );

  /** 历史分页：已加载窗口中最旧消息的 sort_order 游标（后端窗口为降序）。 */
  const oldestSortOrderOf = (messages: DesktopMessage[]): number | null => {
    const orders = messages
      .map((message) => message.sortOrder)
      .filter((value): value is number => typeof value === "number");
    return orders.length > 0 ? Math.min(...orders) : null;
  };

  /** 首次打开/切换会话：只加载最近窗口，不默认一次性加载无限历史。 */
  const loadSessionHistoryWindow = useCallback(
    async (sessionId: string) => {
      if (!("__TAURI_INTERNALS__" in window)) return;
      const sessionUiState = getSessionUiState(sessionId);
      if (
        sessionUiState.timelineItems.length > 0 ||
        sessionUiState.isProcessing ||
        sessionUiState.pendingUserQuestion
      ) {
        return;
      }
      setHistoryLoading(true, sessionId);
      setHistoryError(false, sessionId);
      try {
        const page = await sessionsMessagesPage(sessionId, null, 100);
        // 会话守卫：切换后迟到的回包不得覆盖新会话
        if (activeSessionIdRef.current !== sessionId) return;
        const current = getSessionUiState(sessionId);
        if (
          current.timelineItems.length > 0 ||
          current.isProcessing ||
          current.pendingUserQuestion
        ) {
          return;
        }
        // 后端按 sort_order 降序返回；时间线需要升序（旧 → 新）
        setTimelineItems(messagesToTimelineItems([...page.messages].reverse()), sessionId);
        setHasMoreHistory(page.hasMore, sessionId);
        setHistoryCursor(oldestSortOrderOf(page.messages), sessionId);
      } catch (err) {
        console.error(err);
        if (activeSessionIdRef.current === sessionId) {
          setHistoryError(true, sessionId);
        }
      } finally {
        setHistoryLoading(false, sessionId);
      }
    },
    [getSessionUiState, setHistoryLoading, setHistoryError, setHasMoreHistory, setHistoryCursor, setTimelineItems]
  );

  /** 向上翻页：以当前窗口最旧消息为游标加载更早窗口，并前置合并（幂等去重）。 */
  const loadOlderHistory = useCallback(
    async (sessionId: string) => {
      const sessionUiState = getSessionUiState(sessionId);
      if (
        sessionUiState.historyLoading ||
        !sessionUiState.hasMoreHistory ||
        sessionUiState.historyCursor === null
      ) {
        return;
      }
      setHistoryLoading(true, sessionId);
      setHistoryError(false, sessionId);
      try {
        const page = await sessionsMessagesPage(sessionId, sessionUiState.historyCursor, 100);
        if (activeSessionIdRef.current !== sessionId) return;
        const older = messagesToTimelineItems([...page.messages].reverse());
        setTimelineItems((items) => mergeOlderTimelineItems(items, older), sessionId);
        setHasMoreHistory(page.hasMore, sessionId);
        setHistoryCursor(oldestSortOrderOf(page.messages), sessionId);
      } catch (err) {
        console.error(err);
        if (activeSessionIdRef.current === sessionId) {
          setHistoryError(true, sessionId);
        }
      } finally {
        setHistoryLoading(false, sessionId);
      }
    },
    [getSessionUiState, setHistoryLoading, setHistoryError, setHasMoreHistory, setHistoryCursor, setTimelineItems]
  );

  /**
   * 断线恢复：把未终态 turn 的事件从持久化 journal 重放进事件管道。
   * 恢复期先锁定对应会话（isProcessing + currentTurnId），防止用户
   * 在重放完成前发送新 turn；失败保留锁定状态，由 retryReplay 重试。
   */
  const runReplayPhase = useCallback(
    async (runs: RunState[]) => {
      const nonTerminalRuns = runs.filter((run) => !TERMINAL_RUN_STATUSES.has(run.status));
      // 锁定：重放期间不得发送新 turn（cancelling 状态由 watchdog 对账释放）
      for (const run of nonTerminalRuns) {
        if (!run.turnId) continue;
        setIsProcessing(true, run.sessionId);
        setCurrentTurnId(run.turnId, run.sessionId);
      }
      setReplayState({ status: "loading", retryGeneration: 0 });
      const outcome = await replayTurnEvents({
        runs,
        dispatch: (payload) => {
          handleDesktopRuntimeEvent({ ...buildEventDispatchContext(), payload });
          const terminal = isTerminalTurnEvent(payload);
          if (terminal) {
            registryRef.current.stop(terminal.sessionId, terminal.turnId);
            runsList()
              .then(setRuns)
              .catch((err) => console.warn("[replay] 终态事件后刷新运行状态失败:", err));
          }
        }
      });
      if (outcome.ok) {
        setReplayState({ status: "done", retryGeneration: 0 });
        // 重放完成：有遗留 cancelling 状态的会话恢复取消对账轮询
        for (const run of nonTerminalRuns) {
          if (run.status !== "cancelling") continue;
          registryRef.current
            .resume(run.sessionId, run.turnId, createCancellationWatchdog)
            ?.start();
        }
      } else {
        setReplayState({ status: "error", error: outcome.error, retryGeneration: 0 });
      }
    },
    [buildEventDispatchContext, createCancellationWatchdog, setIsProcessing, setCurrentTurnId, setReplayState]
  );

  /** 初始化序列：注册事件监听 → bootstrap → 运行状态 → 事件重放 → 放行新 turn。 */
  const initializeDesktopRuntime = useCallback(
    async (disposeListener: () => void) => {
      refreshProviderCache();
      let nextBootstrap: Bootstrap;
      try {
        nextBootstrap = await appGetBootstrap();
      } catch (err) {
        console.error(err);
        setBootstrap(fallbackBootstrap);
        if (!("__TAURI_INTERNALS__" in window)) {
          setSessionItems([]);
          activeSessionIdRef.current = null;
          setActiveSessionId(null);
          setSelectedProjectRoot((current) =>
            current === undefined ? fallbackBootstrap.workspacePath : current
          );
          setTimelineItems([]);
        }
        disposeListener();
        return;
      }
      setBootstrap(nextBootstrap);
      // effectivePermissionMode 是后端唯一权威状态；兼容字段 permissionMode
      // 不得被前端用来推导或覆盖实际权限。
      setPermissionMode(nextBootstrap.effectivePermissionMode);
      setSelectedProjectRoot((current) =>
        current === undefined || current === fallbackBootstrap.workspacePath
          ? nextBootstrap.workspacePath
          : current
      );

      const activeSessions = visibleSessions(nextBootstrap.sessions);
      const activeSessionId = activeSessions.find((session) => session.active)?.id ?? null;

      setSessionItems(activeSessions);
      setProjectRoots((current) =>
        dedupeProjectRoots([
          ...current,
          ...activeSessions.map((session) => session.projectRoot),
        ])
      );
      activeSessionIdRef.current = activeSessionId;
      setActiveSessionId(activeSessionId);

      // 运行状态：数据库 turn journal 是事实来源
      let runs: RunState[];
      try {
        runs = await runsList();
      } catch (err) {
        console.error(err);
        setReplayState({
          status: "error",
          error: `获取运行状态失败: ${String(err)}`,
          retryGeneration: 0
        });
        disposeListener();
        return;
      }
      setRuns(runs);

      // 事件重放：未终态 turn 按 lastSeq 恢复；完成后才允许用户发送新 turn
      await runReplayPhase(runs);

      // 无未终态运行的会话：直接加载最近消息窗口
      if (activeSessionId) {
        const activeRun = runs.find((run) => run.sessionId === activeSessionId);
        if (!activeRun || TERMINAL_RUN_STATUSES.has(activeRun.status)) {
          void loadSessionHistoryWindow(activeSessionId);
        }
      }
    },
    [runReplayPhase, loadSessionHistoryWindow, setRuns, setReplayState, setBootstrap, setPermissionMode, setSelectedProjectRoot, setSessionItems, setProjectRoots, setActiveSessionId, setTimelineItems]
  );

  // 初始化：监听 → bootstrap → 运行状态 → 重放（监听先就绪，重放事件不丢失）
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) {
      setBootstrap(fallbackBootstrap);
      setSessionItems([]);
      activeSessionIdRef.current = null;
      setActiveSessionId(null);
      setSelectedProjectRoot((current) =>
        current === undefined ? fallbackBootstrap.workspacePath : current
      );
      setTimelineItems([]);
      return;
    }

    let active = true;
    let disposeFn: (() => void) | undefined;

    // 1. 先注册事件监听；就绪后才允许 bootstrap/重放/发送
    listen<DesktopEvent>("desktop-event", (event) => {
      if (!active) return;
      handleDesktopRuntimeEvent({
        ...buildEventDispatchContext(),
        payload: event.payload
      });
      // 同一 turn 的后端终态事件到达：立即停止取消轮询并清理登记，
      // 避免重复解锁或空转；并刷新 runs 让 RunInspector 反映持久化终态。
      const terminal = isTerminalTurnEvent(event.payload);
      if (terminal) {
        registryRef.current.stop(terminal.sessionId, terminal.turnId);
        runsList()
          .then(setRuns)
          .catch((err) => console.warn("[desktop-event] 终态事件后刷新运行状态失败:", err));
      }
    })
      .then((dispose) => {
        if (!active) {
          dispose();
          return;
        }
        disposeFn = dispose;
        // 2. bootstrap → 运行状态 → 重放
        void initializeDesktopRuntime(() => {
          if (disposeFn) {
            disposeFn();
            disposeFn = undefined;
          }
        });
      })
      .catch((err) => {
        console.error(err);
        void initializeDesktopRuntime(() => {});
      });

    return () => {
      active = false;
      if (disposeFn) {
        disposeFn();
      }
      // 组件卸载：清理全部取消轮询定时器与登记
      registryRef.current.stopAll();
    };
  }, [buildEventDispatchContext, initializeDesktopRuntime, getSessionUiState, sendSystemNotification, createCancellationWatchdog]);

  // 重放失败后的重试路径：保留当前锁定状态，仅重新执行重放阶段
  useEffect(() => {
    const replayState = useAppUiStore.getState().replayState;
    if (replayState.status !== "idle" || replayState.retryGeneration === 0) return;
    const runs = useAppUiStore.getState().runs;
    void runReplayPhase(runs).then(() => {
      const activeRun = runs.find((run) => run.sessionId === activeSessionIdRef.current);
      if (!activeRun || TERMINAL_RUN_STATUSES.has(activeRun.status)) {
        if (activeSessionIdRef.current) {
          void loadSessionHistoryWindow(activeSessionIdRef.current);
        }
      }
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [retryReplayGeneration, runReplayPhase]);

  // 切换会话：暂停被离开会话的取消轮询定时器（保留 pending 登记）；
  // 切回时若该会话仍处于“已取消未终态”状态，则重新武装对账轮询
  // （双重隔离由 watchdog 守卫，旧 watchdog 绝不解锁新任务）。
  const previousActiveSessionIdRef = useRef<string | null>(null);
  useEffect(() => {
    const previous = previousActiveSessionIdRef.current;
    previousActiveSessionIdRef.current = activeSessionId;
    if (previous !== null && previous !== activeSessionId) {
      registryRef.current.suspendSession(previous);
    }
  }, [activeSessionId]);

  useEffect(() => {
    if (!activeSessionId) return;
    const ui = getSessionUiState(activeSessionId);
    if (!ui.isProcessing || !ui.currentTurnId) return;
    registryRef.current
      .resume(activeSessionId, ui.currentTurnId, createCancellationWatchdog)
      ?.start();
  }, [activeSessionId, getSessionUiState, createCancellationWatchdog]);

  return {
    activeSessionIdRef,
    createCancellationWatchdog,
    beginCancellationWatch,
    stopSessionWatchdogs,
    stopTurnWatchdog,
    updateCancellationBoundary,
    loadSessionHistoryWindow,
    loadOlderHistory,
    refreshDesktopRuntime: () => void initializeDesktopRuntime(() => {})
  };
}

function refreshProviderCache() {
  if (!("__TAURI_INTERNALS__" in window)) {
    return;
  }
  configGetProviders()
    .then((providers) => {
      if (Array.isArray(providers)) {
        saveStoredProviders(providers);
      }
    })
    .catch(console.error);
}
