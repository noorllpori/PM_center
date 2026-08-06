import { createContext, createElement, useContext, type ReactNode } from 'react';
import { useStore } from 'zustand';
import { useShallow } from 'zustand/react/shallow';
import { createStore } from 'zustand/vanilla';
import { openStandaloneImageViewer } from '../components/image-viewer/openStandaloneImageViewer';
import { openStandaloneTextEditor } from '../components/text-editor/openStandaloneTextEditor';
import type { TextEditorTransferPayload } from '../components/text-editor/textEditorWindowTransfer';
import { openStandaloneVideoPlayer } from '../components/video-player/openStandaloneVideoPlayer';
import {
  getFileNameFromPath,
  getWorkspaceOpenTarget,
  type WorkspaceOpenTarget,
} from '../components/workspace/fileOpeners';
import { openStandaloneDirectoryViewer } from '../components/file-manager/openStandaloneDirectoryViewer';
import type { ImageSequenceInfo } from '../types';
import {
  WORKSPACE_TAB_CONTRIBUTION_BY_ID,
  WORKSPACE_TAB_CONTRIBUTIONS,
  type WorkspaceTabContributionDefinition,
} from '../features/contributionRegistry';

export type WorkspaceTabType = 'files' | 'cache' | 'render' | 'p2p' | 'contribution' | 'directory' | 'image' | 'text' | 'video' | 'blend' | 'collection';

export type WorkspaceCollectionTabData =
  | {
      kind: 'manual_collection';
      id: string;
      title: string;
      projectPath: string;
      directoryPath: string;
    }
  | {
      kind: 'image_sequence';
      title: string;
      sequence: ImageSequenceInfo;
    };

export interface WorkspaceTab {
  id: string;
  type: WorkspaceTabType;
  title: string;
  closable: boolean;
  filePath?: string;
  isDirty?: boolean;
  editorSnapshot?: TextEditorTransferPayload;
  collection?: WorkspaceCollectionTabData;
  contributionId?: string;
  moduleId?: string;
}

export const FILES_TAB_ID = 'files';
export const CACHE_MANAGER_TAB_ID = WORKSPACE_TAB_CONTRIBUTIONS.cache.tabId;
export const RENDER_CENTER_TAB_ID = WORKSPACE_TAB_CONTRIBUTIONS.render.tabId;
export const P2P_TAB_ID = WORKSPACE_TAB_CONTRIBUTIONS.p2p.tabId;

const FILES_TAB: WorkspaceTab = {
  id: FILES_TAB_ID,
  type: 'files',
  title: '文件列表',
  closable: false,
};

const CACHE_MANAGER_TAB: WorkspaceTab = {
  id: CACHE_MANAGER_TAB_ID,
  type: 'cache',
  title: '缓存管理',
  closable: true,
  contributionId: WORKSPACE_TAB_CONTRIBUTIONS.cache.id,
  moduleId: WORKSPACE_TAB_CONTRIBUTIONS.cache.moduleId || undefined,
};

const RENDER_CENTER_TAB: WorkspaceTab = {
  id: RENDER_CENTER_TAB_ID,
  type: 'render',
  title: '渲染与批处理',
  closable: true,
  contributionId: WORKSPACE_TAB_CONTRIBUTIONS.render.id,
  moduleId: WORKSPACE_TAB_CONTRIBUTIONS.render.moduleId || undefined,
};

const P2P_TAB: WorkspaceTab = {
  id: P2P_TAB_ID,
  type: 'p2p',
  title: '局域网项目功能',
  closable: true,
  contributionId: WORKSPACE_TAB_CONTRIBUTIONS.p2p.id,
  moduleId: WORKSPACE_TAB_CONTRIBUTIONS.p2p.moduleId || undefined,
};

function createWorkspaceContributionTab(
  definition: WorkspaceTabContributionDefinition,
): WorkspaceTab {
  return {
    id: definition.tabId,
    type: definition.tabType,
    title: definition.title,
    closable: true,
    contributionId: definition.id,
    moduleId: definition.moduleId || undefined,
  };
}

function createFileTab(
  type: 'image' | 'text' | 'video' | 'blend',
  filePath: string,
  editorSnapshot?: TextEditorTransferPayload,
): WorkspaceTab {
  return {
    id: `${type}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    type,
    title: getFileNameFromPath(filePath),
    closable: true,
    filePath,
    isDirty: false,
    editorSnapshot: type === 'text' ? editorSnapshot : undefined,
  };
}

function createDirectoryTab(directoryPath: string): WorkspaceTab {
  return {
    id: `directory-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    type: 'directory',
    title: getFileNameFromPath(directoryPath) || '目录',
    closable: true,
    filePath: directoryPath,
    isDirty: false,
  };
}

function createCollectionTab(collection: WorkspaceCollectionTabData): WorkspaceTab {
  const identity =
    collection.kind === 'manual_collection'
      ? collection.id
      : collection.sequence.virtual_path;

  return {
    id: `collection-${collection.kind}-${identity}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    type: 'collection',
    title: collection.title,
    closable: true,
    filePath: identity,
    isDirty: false,
    collection,
  };
}

export interface WorkspaceTabState {
  tabs: WorkspaceTab[];
  activeTabId: string;
  openFileInTab: (
    filePath: string,
    options?: {
      editorSnapshot?: TextEditorTransferPayload;
      workspaceTarget?: WorkspaceOpenTarget;
    },
  ) => Promise<string | null>;
  openFileInStandaloneWindow: (
    filePath: string,
    options?: {
      projectPath?: string;
      title?: string;
    },
  ) => Promise<boolean>;
  openDirectoryInStandaloneWindow: (
    directoryPath: string,
    options?: {
      projectPath?: string;
      projectName?: string;
      title?: string;
    },
  ) => Promise<boolean>;
  openDirectoryInTab: (directoryPath: string) => string;
  openWorkspaceContributionTab: (contributionId: string) => string | null;
  openCacheManagerTab: () => string;
  openRenderCenterTab: () => string;
  openP2PTab: () => string;
  openCollectionInTab: (collection: WorkspaceCollectionTabData) => string;
  activateTab: (tabId: string) => void;
  closeTab: (tabId: string) => void;
  closeContributionTabs: (contributionIds: string[]) => void;
  reorderTabs: (fromId: string, toId: string) => void;
  updateTabDirty: (tabId: string, isDirty: boolean) => void;
  resetTabs: () => void;
}

interface CreateWorkspaceTabStoreOptions {
  forceStandaloneFileOpen?: boolean;
  standaloneProjectPath?: string;
  standaloneProjectName?: string;
}

export function createWorkspaceTabStore(storeOptions: CreateWorkspaceTabStoreOptions = {}) {
  return createStore<WorkspaceTabState>((set, get) => ({
    tabs: [FILES_TAB],
    activeTabId: FILES_TAB_ID,

    openFileInTab: async (filePath, options) => {
      const target = options?.workspaceTarget ?? getWorkspaceOpenTarget(filePath);
      if (!target) {
        return null;
      }

      if (storeOptions.forceStandaloneFileOpen) {
        if (target === 'blend') {
          return null;
        }

        const opened = await get().openFileInStandaloneWindow(filePath, {
          projectPath: storeOptions.standaloneProjectPath,
        });
        return opened ? `standalone-${target}:${filePath}` : null;
      }

      const existingTab = get().tabs.find(
        (tab) => tab.type === target && tab.filePath === filePath,
      );

      if (existingTab) {
        if (options?.editorSnapshot && existingTab.type === 'text') {
          set((state) => ({
            tabs: state.tabs.map((tab) =>
              tab.id === existingTab.id
                ? { ...tab, editorSnapshot: options.editorSnapshot }
                : tab,
            ),
            activeTabId: existingTab.id,
          }));
        } else {
          set({ activeTabId: existingTab.id });
        }
        return existingTab.id;
      }

      const nextTab = createFileTab(target, filePath, options?.editorSnapshot);
      set((state) => ({
        tabs: [...state.tabs, nextTab],
        activeTabId: nextTab.id,
      }));

      return nextTab.id;
    },

    openFileInStandaloneWindow: async (filePath, options) => {
      const target = getWorkspaceOpenTarget(filePath);
      if (!target) {
        return false;
      }

      if (target === 'image') {
        await openStandaloneImageViewer({
          filePath,
          title: options?.title,
          projectPath: options?.projectPath,
        });
        return true;
      }

      if (target === 'video') {
        await openStandaloneVideoPlayer({
          filePath,
          title: options?.title,
          projectPath: options?.projectPath,
        });
        return true;
      }

      if (target === 'blend') {
        return false;
      }

      await openStandaloneTextEditor({
        filePath,
        title: options?.title,
        projectPath: options?.projectPath,
      });
      return true;
    },

    openDirectoryInStandaloneWindow: async (directoryPath, options) => {
      await openStandaloneDirectoryViewer({
        directoryPath,
        title: options?.title,
        projectPath: options?.projectPath,
        projectName: options?.projectName,
      });
      return true;
    },

    openDirectoryInTab: (directoryPath) => {
      if (storeOptions.forceStandaloneFileOpen) {
        void get().openDirectoryInStandaloneWindow(directoryPath, {
          projectPath: storeOptions.standaloneProjectPath,
          projectName: storeOptions.standaloneProjectName,
        });
        return `standalone-directory:${directoryPath}`;
      }

      const existingTab = get().tabs.find(
        (tab) => tab.type === 'directory' && tab.filePath === directoryPath,
      );

      if (existingTab) {
        set({ activeTabId: existingTab.id });
        return existingTab.id;
      }

      const nextTab = createDirectoryTab(directoryPath);
      set((state) => ({
        tabs: [...state.tabs, nextTab],
        activeTabId: nextTab.id,
      }));

      return nextTab.id;
    },

    openWorkspaceContributionTab: (contributionId) => {
      const definition = WORKSPACE_TAB_CONTRIBUTION_BY_ID.get(contributionId);
      if (!definition) {
        return null;
      }
      if (storeOptions.forceStandaloneFileOpen) {
        return definition.tabId;
      }

      const existingTab = get().tabs.find(
        (tab) => tab.contributionId === contributionId || tab.id === definition.tabId,
      );
      if (existingTab) {
        set({ activeTabId: existingTab.id });
        return existingTab.id;
      }

      const nextTab = createWorkspaceContributionTab(definition);
      set((state) => ({
        tabs: [...state.tabs, nextTab],
        activeTabId: nextTab.id,
      }));
      return nextTab.id;
    },

    openCacheManagerTab: () => {
      return get().openWorkspaceContributionTab(WORKSPACE_TAB_CONTRIBUTIONS.cache.id)
        ?? CACHE_MANAGER_TAB.id;
    },

    openRenderCenterTab: () => {
      return get().openWorkspaceContributionTab(WORKSPACE_TAB_CONTRIBUTIONS.render.id)
        ?? RENDER_CENTER_TAB.id;
    },

    openP2PTab: () => {
      return get().openWorkspaceContributionTab(WORKSPACE_TAB_CONTRIBUTIONS.p2p.id)
        ?? P2P_TAB.id;
    },

    openCollectionInTab: (collection) => {
      const identity =
        collection.kind === 'manual_collection'
          ? collection.id
          : collection.sequence.virtual_path;
      const existingTab = get().tabs.find(
        (tab) =>
          tab.type === 'collection' &&
          tab.collection?.kind === collection.kind &&
          tab.filePath === identity,
      );

      if (existingTab) {
        set({ activeTabId: existingTab.id });
        return existingTab.id;
      }

      const nextTab = createCollectionTab(collection);
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
          activeTabId: removedActiveTab ? FILES_TAB_ID : state.activeTabId,
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
      set((state) => {
        let hasChanged = false;

        const nextTabs = state.tabs.map((tab) => {
          if (tab.id !== tabId) {
            return tab;
          }

          if ((tab.isDirty ?? false) === isDirty) {
            return tab;
          }

          hasChanged = true;
          return { ...tab, isDirty };
        });

        if (!hasChanged) {
          return state;
        }

        return {
          tabs: nextTabs,
        };
      });
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
