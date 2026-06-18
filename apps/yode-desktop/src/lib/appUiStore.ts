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
  inspectorWidth: number;
  sidebarWidth: number;
  terminalHeight: number;
  viewMode: ViewMode;
  setAppLang: (lang: string) => void;
  setInspectorWidth: (width: number) => void;
  setSidebarWidth: (width: number) => void;
  setTerminalHeight: (height: number) => void;
  setViewMode: (mode: ViewMode) => void;
};

function storedViewMode(): ViewMode {
  const raw = localStorage.getItem("yode-view-mode");
  return raw === "settings" ? "settings" : "chat";
}

export const useAppUiStore = create<AppUiState>((set) => ({
  appLang: localStorage.getItem("yode-language") || "zh",
  inspectorWidth: loadInitialPaneSize("inspector", INSPECTOR_WIDTH_STORAGE_KEY),
  sidebarWidth: loadInitialPaneSize("sidebar", SIDEBAR_WIDTH_STORAGE_KEY),
  terminalHeight: loadInitialPaneSize("terminal", TERMINAL_HEIGHT_STORAGE_KEY),
  viewMode: storedViewMode(),
  setAppLang: (appLang) => {
    localStorage.setItem("yode-language", appLang);
    set({ appLang });
  },
  setInspectorWidth: (inspectorWidth) => {
    localStorage.setItem(INSPECTOR_WIDTH_STORAGE_KEY, String(inspectorWidth));
    set({ inspectorWidth });
  },
  setSidebarWidth: (sidebarWidth) => {
    localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(sidebarWidth));
    set({ sidebarWidth });
  },
  setTerminalHeight: (terminalHeight) => {
    localStorage.setItem(TERMINAL_HEIGHT_STORAGE_KEY, String(terminalHeight));
    set({ terminalHeight });
  },
  setViewMode: (viewMode) => {
    localStorage.setItem("yode-view-mode", viewMode);
    set({ viewMode });
  }
}));
