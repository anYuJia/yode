import { create } from "zustand";

import type { ViewMode } from "../components/Sidebar";
import {
  INSPECTOR_WIDTH_STORAGE_KEY,
  loadInitialPaneSize,
  SIDEBAR_WIDTH_STORAGE_KEY,
  TERMINAL_HEIGHT_STORAGE_KEY
} from "./paneLayout";

type AppUiState = {
  appLang: string;
  inspectorOpen: boolean;
  inspectorWidth: number;
  sidebarOpen: boolean;
  sidebarWidth: number;
  terminalHeight: number;
  terminalOpenByConversation: Record<string, boolean>;
  viewMode: ViewMode;
  setAppLang: (lang: string) => void;
  setInspectorOpen: (open: boolean) => void;
  setInspectorWidth: (width: number) => void;
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

export const useAppUiStore = create<AppUiState>((set) => ({
  appLang: localStorage.getItem("yode-language") || "zh",
  inspectorOpen: true,
  inspectorWidth: loadInitialPaneSize("inspector", INSPECTOR_WIDTH_STORAGE_KEY),
  sidebarOpen: true,
  sidebarWidth: loadInitialPaneSize("sidebar", SIDEBAR_WIDTH_STORAGE_KEY),
  terminalHeight: loadInitialPaneSize("terminal", TERMINAL_HEIGHT_STORAGE_KEY),
  terminalOpenByConversation: {},
  viewMode: storedViewMode(),
  setAppLang: (appLang) => {
    localStorage.setItem("yode-language", appLang);
    set({ appLang });
  },
  setInspectorOpen: (inspectorOpen) => set({ inspectorOpen }),
  setInspectorWidth: (inspectorWidth) => {
    localStorage.setItem(INSPECTOR_WIDTH_STORAGE_KEY, String(inspectorWidth));
    set({ inspectorWidth });
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
