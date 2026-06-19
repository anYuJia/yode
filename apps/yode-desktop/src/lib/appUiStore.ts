import { create } from "zustand";

import type { ViewMode } from "../components/Sidebar";
import {
  INSPECTOR_WIDTH_STORAGE_KEY,
  loadInitialPaneSize,
  SIDEBAR_WIDTH_STORAGE_KEY,
  TERMINAL_HEIGHT_STORAGE_KEY
} from "./paneLayout";
import {
  loadStoredProjectOrder,
  loadStoredProjectRoots,
  loadStoredSelectedProjectRoot,
  PROJECT_ORDER_STORAGE_KEY,
  PROJECT_ROOTS_STORAGE_KEY,
  SELECTED_PROJECT_ROOT_STORAGE_KEY,
  STANDALONE_PROJECT_SENTINEL
} from "./projectStorage";

type StateUpdater<T> = T | ((current: T) => T);

type AppUiState = {
  appLang: string;
  inspectorOpen: boolean;
  inspectorWidth: number;
  projectOrder: string[];
  projectRoots: string[];
  selectedProjectRoot: string | null | undefined;
  sidebarOpen: boolean;
  sidebarWidth: number;
  terminalHeight: number;
  terminalOpenByConversation: Record<string, boolean>;
  viewMode: ViewMode;
  reloadProjectStorage: () => void;
  setAppLang: (lang: string) => void;
  setInspectorOpen: (open: boolean) => void;
  setInspectorWidth: (width: number) => void;
  setProjectOrder: (order: StateUpdater<string[]>) => void;
  setProjectRoots: (roots: StateUpdater<string[]>) => void;
  setSelectedProjectRoot: (root: StateUpdater<string | null | undefined>) => void;
  setSidebarOpen: (open: boolean) => void;
  setSidebarWidth: (width: number) => void;
  setTerminalHeight: (height: number) => void;
  setTerminalOpenForConversation: (conversationKey: string, open: boolean) => void;
  setViewMode: (mode: ViewMode) => void;
};

function storedViewMode(): ViewMode {
  const raw = localStorage.getItem("yode-view-mode");
  return raw === "settings" ? "settings" : "chat";
}

function resolveUpdater<T>(updater: StateUpdater<T>, current: T): T {
  return typeof updater === "function"
    ? (updater as (current: T) => T)(current)
    : updater;
}

export const useAppUiStore = create<AppUiState>((set, get) => ({
  appLang: localStorage.getItem("yode-language") || "zh",
  inspectorOpen: true,
  inspectorWidth: loadInitialPaneSize("inspector", INSPECTOR_WIDTH_STORAGE_KEY),
  projectOrder: loadStoredProjectOrder(),
  projectRoots: loadStoredProjectRoots(),
  selectedProjectRoot: loadStoredSelectedProjectRoot(),
  sidebarOpen: true,
  sidebarWidth: loadInitialPaneSize("sidebar", SIDEBAR_WIDTH_STORAGE_KEY),
  terminalHeight: loadInitialPaneSize("terminal", TERMINAL_HEIGHT_STORAGE_KEY),
  terminalOpenByConversation: {},
  viewMode: storedViewMode(),
  reloadProjectStorage: () => set({
    projectOrder: loadStoredProjectOrder(),
    projectRoots: loadStoredProjectRoots(),
    selectedProjectRoot: loadStoredSelectedProjectRoot(),
  }),
  setAppLang: (appLang) => {
    localStorage.setItem("yode-language", appLang);
    set({ appLang });
  },
  setInspectorOpen: (inspectorOpen) => set({ inspectorOpen }),
  setInspectorWidth: (inspectorWidth) => {
    localStorage.setItem(INSPECTOR_WIDTH_STORAGE_KEY, String(inspectorWidth));
    set({ inspectorWidth });
  },
  setProjectOrder: (updater) => {
    const projectOrder = resolveUpdater(updater, get().projectOrder);
    localStorage.setItem(PROJECT_ORDER_STORAGE_KEY, JSON.stringify(projectOrder));
    set({ projectOrder });
  },
  setProjectRoots: (updater) => {
    const projectRoots = resolveUpdater(updater, get().projectRoots);
    localStorage.setItem(PROJECT_ROOTS_STORAGE_KEY, JSON.stringify(projectRoots));
    set({ projectRoots });
  },
  setSelectedProjectRoot: (updater) => {
    const selectedProjectRoot = resolveUpdater(updater, get().selectedProjectRoot);
    if (selectedProjectRoot !== undefined) {
      localStorage.setItem(
        SELECTED_PROJECT_ROOT_STORAGE_KEY,
        selectedProjectRoot === null ? STANDALONE_PROJECT_SENTINEL : selectedProjectRoot
      );
    }
    set({ selectedProjectRoot });
  },
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
  setSidebarWidth: (sidebarWidth) => {
    localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(sidebarWidth));
    set({ sidebarWidth });
  },
  setTerminalHeight: (terminalHeight) => {
    localStorage.setItem(TERMINAL_HEIGHT_STORAGE_KEY, String(terminalHeight));
    set({ terminalHeight });
  },
  setTerminalOpenForConversation: (conversationKey, open) => set((state) => ({
    terminalOpenByConversation: {
      ...state.terminalOpenByConversation,
      [conversationKey]: open
    }
  })),
  setViewMode: (viewMode) => {
    localStorage.setItem("yode-view-mode", viewMode);
    set({ viewMode });
  }
}));
