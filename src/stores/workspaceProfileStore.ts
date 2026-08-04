import { create } from 'zustand';
import {
  getWorkspaceProfileRuntime,
  initializeWorkspaceProfileRuntime,
  previewWorkspaceProfileSwitch,
  switchWorkspaceProfile,
} from '../api/workspaceProfiles';
import { PLATFORM_MODULE_RUNTIME_CHANGED_EVENT } from '../api/platformModules';
import {
  getKnownToolContributionIds,
  getPinnedToolContributionIds,
  useBuiltinToolsStore,
} from './builtinToolsStore';
import type {
  WorkspaceProfileRuntimeCommandError,
  WorkspaceProfileRuntimeSnapshot,
  WorkspaceProfileSwitchPreview,
} from '../types/workspaceProfileRuntime';

interface WorkspaceProfileState {
  snapshot: WorkspaceProfileRuntimeSnapshot | null;
  isInitialized: boolean;
  isLoading: boolean;
  isSwitching: boolean;
  error: string | null;
  switchPreview: WorkspaceProfileSwitchPreview | null;
  switchMessage: string | null;
  initialize: (legacyPinnedTools: string[]) => Promise<void>;
  refresh: () => Promise<void>;
  previewSwitch: (profileId: string) => Promise<void>;
  switchProfile: (profileId: string) => Promise<void>;
  clearSwitchPreview: () => void;
}

let initializationPromise: Promise<void> | null = null;
let refreshPromise: Promise<void> | null = null;

function formatRuntimeError(error: unknown) {
  if (typeof error === 'string') {
    return error;
  }
  if (error && typeof error === 'object') {
    const typed = error as WorkspaceProfileRuntimeCommandError;
    const prefix = typed.code ? `${typed.code}: ` : '';
    const suffix = typed.path ? `\n${typed.path}` : '';
    const details = typed.details?.length ? `\n${typed.details.join('\n')}` : '';
    return `${prefix}${typed.message || String(error)}${details}${suffix}`;
  }
  return String(error);
}

export const useWorkspaceProfileStore = create<WorkspaceProfileState>((set, get) => ({
  snapshot: null,
  isInitialized: false,
  isLoading: false,
  isSwitching: false,
  error: null,
  switchPreview: null,
  switchMessage: null,

  initialize: async (legacyPinnedTools) => {
    if (get().isInitialized) {
      return;
    }
    if (initializationPromise) {
      return initializationPromise;
    }
    initializationPromise = (async () => {
      set({ isLoading: true, error: null });
      try {
        const snapshot = await initializeWorkspaceProfileRuntime(legacyPinnedTools);
        const currentSummary = snapshot.profiles.find((profile) => profile.current);
        if (currentSummary?.status === 'ready') {
          await useBuiltinToolsStore
            .getState()
            .replacePinnedByContributionIds(snapshot.currentProfile.shellLayout?.pinnedTools ?? []);
        }
        set({ snapshot, isInitialized: true, error: null });
      } catch (error) {
        set({ isInitialized: true, error: formatRuntimeError(error) });
      } finally {
        set({ isLoading: false });
        initializationPromise = null;
      }
    })();
    return initializationPromise;
  },

  refresh: async () => {
    if (refreshPromise) {
      return refreshPromise;
    }
    refreshPromise = (async () => {
      set({ isLoading: true, error: null });
      try {
        const snapshot = await getWorkspaceProfileRuntime();
        set({ snapshot, isInitialized: true, error: null });
      } catch (error) {
        set({ error: formatRuntimeError(error) });
      } finally {
        set({ isLoading: false });
        refreshPromise = null;
      }
    })();
    return refreshPromise;
  },

  previewSwitch: async (profileId) => {
    set({ isLoading: true, error: null, switchMessage: null });
    try {
      const preview = await previewWorkspaceProfileSwitch({
        profileId,
        currentPinnedTools: getPinnedToolContributionIds(),
        knownToolContributions: getKnownToolContributionIds(),
      });
      set({ switchPreview: preview, error: null });
    } catch (error) {
      set({ error: formatRuntimeError(error), switchPreview: null });
    } finally {
      set({ isLoading: false });
    }
  },

  switchProfile: async (profileId) => {
    const currentSnapshot = get().snapshot;
    if (!currentSnapshot || get().isSwitching) {
      return;
    }
    const previousProfileId = currentSnapshot.currentProfile.id;
    const previousPinnedTools = getPinnedToolContributionIds();
    const knownToolContributions = getKnownToolContributionIds();
    set({ isSwitching: true, error: null, switchMessage: null });
    try {
      const result = await switchWorkspaceProfile({
        profileId,
        expectedCurrentProfileId: previousProfileId,
        currentPinnedTools: previousPinnedTools,
        knownToolContributions,
      });
      try {
        await useBuiltinToolsStore
          .getState()
          .replacePinnedByContributionIds(result.preview.pinnedToolsAfter);
      } catch (pinError) {
        try {
          await switchWorkspaceProfile({
            profileId: previousProfileId,
            expectedCurrentProfileId: result.snapshot.currentProfile.id,
            currentPinnedTools: previousPinnedTools,
            knownToolContributions,
          });
        } catch (rollbackError) {
          throw new Error(
            `快捷栏写入失败，Profile 回滚也失败：${formatRuntimeError(pinError)}\n${formatRuntimeError(rollbackError)}`,
          );
        }
        throw new Error(`快捷栏写入失败，已恢复原 Profile：${formatRuntimeError(pinError)}`);
      }

      set({
        snapshot: result.snapshot,
        switchPreview: null,
        switchMessage: `已切换到“${result.snapshot.currentProfile.name}”`,
        error: null,
      });
      window.dispatchEvent(new Event(PLATFORM_MODULE_RUNTIME_CHANGED_EVENT));
    } catch (error) {
      set({ error: formatRuntimeError(error) });
      try {
        const snapshot = await getWorkspaceProfileRuntime();
        set({ snapshot });
      } catch {
        // Keep the original switch error visible.
      }
    } finally {
      set({ isSwitching: false });
    }
  },

  clearSwitchPreview: () => set({ switchPreview: null, switchMessage: null }),
}));
