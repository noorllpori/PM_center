import { create } from 'zustand';

export type ShellTabType = 'home' | 'project';

export interface ShellTab {
  id: string;
  type: ShellTabType;
  title: string;
  closable: boolean;
  projectPath?: string;
  normalizedProjectPath?: string;
}

const HOME_TAB_ID = 'home';

const HOME_TAB: ShellTab = {
  id: HOME_TAB_ID,
  type: 'home',
  title: '主页',
  closable: false,
};

export function normalizeProjectPath(path: string) {
  return path
    .replace(/[\\/]+/g, '/')
    .replace(/\/$/, '')
    .toLowerCase();
}

interface ShellTabState {
  tabs: ShellTab[];
  activeTabId: string;
  openProjectTab: (projectPath: string, title: string) => string;
  activateTab: (tabId: string) => void;
  closeTab: (tabId: string) => void;
  reorderTabs: (fromId: string, toId: string) => void;
  findProjectTab: (projectPath: string) => ShellTab | undefined;
}

export const useShellTabStore = create<ShellTabState>()((set, get) => ({
  tabs: [HOME_TAB],
  activeTabId: HOME_TAB_ID,

  openProjectTab: (projectPath, title) => {
    const normalizedProjectPath = normalizeProjectPath(projectPath);
    const existingTab = get().tabs.find(
      (tab) => tab.type === 'project' && tab.normalizedProjectPath === normalizedProjectPath,
    );

    if (existingTab) {
      set({ activeTabId: existingTab.id });
      return existingTab.id;
    }

    const nextTab: ShellTab = {
      id: `project-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      type: 'project',
      title,
      closable: true,
      projectPath,
      normalizedProjectPath,
    };

    set((state) => ({
      tabs: [...state.tabs, nextTab],
      activeTabId: nextTab.id,
    }));

    return nextTab.id;
  },

  activateTab: (tabId) => {
    if (!get().tabs.some((tab) => tab.id === tabId)) {
      return;
    }

    set({ activeTabId: tabId });
  },

  closeTab: (tabId) => {
    if (tabId === HOME_TAB_ID) {
      return;
    }

    set((state) => {
      const currentIndex = state.tabs.findIndex((tab) => tab.id === tabId);
      if (currentIndex < 0) {
        return state;
      }

      const nextTabs = state.tabs.filter((tab) => tab.id !== tabId);
      let nextActiveTabId = state.activeTabId;

      if (state.activeTabId === tabId) {
        const nextNeighbor =
          state.tabs[currentIndex + 1] ??
          state.tabs[currentIndex - 1] ??
          HOME_TAB;
        nextActiveTabId = nextNeighbor.id;
      }

      return {
        tabs: nextTabs,
        activeTabId: nextActiveTabId,
      };
    });
  },

  reorderTabs: (fromId, toId) => {
    if (fromId === toId || fromId === HOME_TAB_ID || toId === HOME_TAB_ID) {
      return;
    }

    set((state) => {
      const dynamicTabs = state.tabs.filter((tab) => tab.id !== HOME_TAB_ID);
      const fromIndex = dynamicTabs.findIndex((tab) => tab.id === fromId);
      const toIndex = dynamicTabs.findIndex((tab) => tab.id === toId);

      if (fromIndex < 0 || toIndex < 0) {
        return state;
      }

      const nextDynamicTabs = [...dynamicTabs];
      const [movedTab] = nextDynamicTabs.splice(fromIndex, 1);
      nextDynamicTabs.splice(toIndex, 0, movedTab);

      return {
        tabs: [HOME_TAB, ...nextDynamicTabs],
      };
    });
  },

  findProjectTab: (projectPath) => {
    const normalizedProjectPath = normalizeProjectPath(projectPath);
    return get().tabs.find(
      (tab) => tab.type === 'project' && tab.normalizedProjectPath === normalizedProjectPath,
    );
  },
}));
