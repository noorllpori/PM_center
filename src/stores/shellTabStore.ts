import { create } from 'zustand';
import {
  SHELL_TAB_CONTRIBUTION_BY_ID,
  SHELL_TAB_CONTRIBUTIONS,
} from '../features/contributionRegistry';

export type ShellTabType = 'home' | 'project' | 'lan';

export interface ShellTab {
  id: string;
  type: ShellTabType;
  title: string;
  closable: boolean;
  projectPath?: string;
  normalizedProjectPath?: string;
  contributionId?: string;
  moduleId?: string;
}

const HOME_TAB_ID = 'home';
export const LAN_SHELL_TAB_ID = SHELL_TAB_CONTRIBUTIONS.lan.tabId;

const HOME_TAB: ShellTab = {
  id: HOME_TAB_ID,
  type: 'home',
  title: '主页',
  closable: false,
};

const LAN_SHELL_TAB: ShellTab = {
  id: LAN_SHELL_TAB_ID,
  type: 'lan',
  title: '设备协作',
  closable: true,
  contributionId: SHELL_TAB_CONTRIBUTIONS.lan.id,
  moduleId: SHELL_TAB_CONTRIBUTIONS.lan.moduleId || undefined,
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
  openShellContributionTab: (contributionId: string) => string | null;
  openLanTab: () => string;
  activateTab: (tabId: string) => void;
  closeTab: (tabId: string) => void;
  closeContributionTabs: (contributionIds: string[]) => void;
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

  openShellContributionTab: (contributionId) => {
    const definition = SHELL_TAB_CONTRIBUTION_BY_ID.get(contributionId);
    if (!definition) {
      return null;
    }
    const existingTab = get().tabs.find(
      (tab) => tab.contributionId === contributionId || tab.id === definition.tabId,
    );
    if (existingTab) {
      set({ activeTabId: existingTab.id });
      return existingTab.id;
    }
    const nextTab: ShellTab = {
      id: definition.tabId,
      type: definition.tabType,
      title: definition.title,
      closable: true,
      contributionId: definition.id,
      moduleId: definition.moduleId || undefined,
    };
    set((state) => ({
      tabs: [...state.tabs, nextTab],
      activeTabId: nextTab.id,
    }));
    return nextTab.id;
  },

  openLanTab: () => {
    return get().openShellContributionTab(SHELL_TAB_CONTRIBUTIONS.lan.id)
      ?? LAN_SHELL_TAB.id;
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

  closeContributionTabs: (contributionIds) => {
    const contributionIdSet = new Set(contributionIds);
    if (contributionIdSet.size === 0) {
      return;
    }
    set((state) => {
      const removedActiveTab = state.tabs.some(
        (tab) => tab.id === state.activeTabId
          && Boolean(tab.contributionId)
          && contributionIdSet.has(tab.contributionId!),
      );
      const nextTabs = state.tabs.filter(
        (tab) => !tab.contributionId || !contributionIdSet.has(tab.contributionId),
      );
      if (nextTabs.length === state.tabs.length) {
        return state;
      }
      return {
        tabs: nextTabs,
        activeTabId: removedActiveTab ? HOME_TAB_ID : state.activeTabId,
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
