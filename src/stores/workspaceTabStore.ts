import { createContext, createElement, useContext, type ReactNode } from 'react';
import { useStore } from 'zustand';
import { useShallow } from 'zustand/react/shallow';
import { createStore } from 'zustand/vanilla';
import { openStandaloneImageViewer } from '../components/image-viewer/openStandaloneImageViewer';
import { openStandaloneTextEditor } from '../components/text-editor/openStandaloneTextEditor';
import { getFileNameFromPath, getWorkspaceOpenTarget } from '../components/workspace/fileOpeners';

export type WorkspaceTabType = 'files' | 'logs' | 'image' | 'text';

export interface WorkspaceTab {
  id: string;
  type: WorkspaceTabType;
  title: string;
  closable: boolean;
  filePath?: string;
  isDirty?: boolean;
}

const FILES_TAB_ID = 'files';
const LOGS_TAB_ID = 'logs';

const FILES_TAB: WorkspaceTab = {
  id: FILES_TAB_ID,
  type: 'files',
  title: '文件列表',
  closable: false,
};

function createFileTab(type: 'image' | 'text', filePath: string): WorkspaceTab {
  return {
    id: `${type}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    type,
    title: getFileNameFromPath(filePath),
    closable: true,
    filePath,
    isDirty: false,
  };
}

export interface WorkspaceTabState {
  tabs: WorkspaceTab[];
  activeTabId: string;
  openFileInTab: (filePath: string) => Promise<string | null>;
  openFileInStandaloneWindow: (filePath: string) => Promise<boolean>;
  openLogsTab: () => string;
  activateTab: (tabId: string) => void;
  closeTab: (tabId: string) => void;
  reorderTabs: (fromId: string, toId: string) => void;
  updateTabDirty: (tabId: string, isDirty: boolean) => void;
  resetTabs: () => void;
}

export function createWorkspaceTabStore() {
  return createStore<WorkspaceTabState>((set, get) => ({
    tabs: [FILES_TAB],
    activeTabId: FILES_TAB_ID,

    openFileInTab: async (filePath) => {
      const target = getWorkspaceOpenTarget(filePath);
      if (!target) {
        return null;
      }

      const existingTab = get().tabs.find(
        (tab) => tab.type === target && tab.filePath === filePath,
      );

      if (existingTab) {
        set({ activeTabId: existingTab.id });
        return existingTab.id;
      }

      const nextTab = createFileTab(target, filePath);
      set((state) => ({
        tabs: [...state.tabs, nextTab],
        activeTabId: nextTab.id,
      }));

      return nextTab.id;
    },

    openFileInStandaloneWindow: async (filePath) => {
      const target = getWorkspaceOpenTarget(filePath);
      if (!target) {
        return false;
      }

      if (target === 'image') {
        await openStandaloneImageViewer(filePath);
        return true;
      }

      await openStandaloneTextEditor(filePath);
      return true;
    },

    openLogsTab: () => {
      const existingTab = get().tabs.find((tab) => tab.id === LOGS_TAB_ID);
      if (existingTab) {
        set({ activeTabId: existingTab.id });
        return existingTab.id;
      }

      const nextTab: WorkspaceTab = {
        id: LOGS_TAB_ID,
        type: 'logs',
        title: '日志',
        closable: true,
        isDirty: false,
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
      if (tabId === FILES_TAB_ID) {
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
            FILES_TAB;
          nextActiveTabId = nextNeighbor.id;
        }

        return {
          tabs: nextTabs,
          activeTabId: nextActiveTabId,
        };
      });
    },

    reorderTabs: (fromId, toId) => {
      if (fromId === toId || fromId === FILES_TAB_ID || toId === FILES_TAB_ID) {
        return;
      }

      set((state) => {
        const dynamicTabs = state.tabs.filter((tab) => tab.id !== FILES_TAB_ID);
        const fromIndex = dynamicTabs.findIndex((tab) => tab.id === fromId);
        const toIndex = dynamicTabs.findIndex((tab) => tab.id === toId);

        if (fromIndex < 0 || toIndex < 0) {
          return state;
        }

        const nextDynamicTabs = [...dynamicTabs];
        const [movedTab] = nextDynamicTabs.splice(fromIndex, 1);
        nextDynamicTabs.splice(toIndex, 0, movedTab);

        return {
          tabs: [FILES_TAB, ...nextDynamicTabs],
        };
      });
    },

    updateTabDirty: (tabId, isDirty) => {
      set((state) => ({
        tabs: state.tabs.map((tab) => (
          tab.id === tabId ? { ...tab, isDirty } : tab
        )),
      }));
    },

    resetTabs: () => {
      set({
        tabs: [FILES_TAB],
        activeTabId: FILES_TAB_ID,
      });
    },
  }));
}

export type WorkspaceTabStoreApi = ReturnType<typeof createWorkspaceTabStore>;

const WorkspaceTabStoreContext = createContext<WorkspaceTabStoreApi | null>(null);

export function WorkspaceTabStoreProvider({
  store,
  children,
}: {
  store: WorkspaceTabStoreApi;
  children: ReactNode;
}) {
  return createElement(WorkspaceTabStoreContext.Provider, { value: store }, children);
}

export function useWorkspaceTabStoreApi() {
  const store = useContext(WorkspaceTabStoreContext);
  if (!store) {
    throw new Error('useWorkspaceTabStoreApi must be used within a WorkspaceTabStoreProvider');
  }
  return store;
}

export function useWorkspaceTabStore<T>(selector: (state: WorkspaceTabState) => T) {
  return useStore(useWorkspaceTabStoreApi(), selector);
}

export function useWorkspaceTabStoreShallow<T>(selector: (state: WorkspaceTabState) => T) {
  return useStore(useWorkspaceTabStoreApi(), useShallow(selector));
}
