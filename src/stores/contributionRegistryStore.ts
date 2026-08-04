import { create } from 'zustand';
import {
  PLATFORM_MODULE_RUNTIME_CHANGED_EVENT,
  getPlatformModuleRuntime,
} from '../api/platformModules';
import {
  EMPTY_CONTRIBUTION_REGISTRY,
  buildContributionRegistry,
  type ContributionRegistrySnapshot,
} from '../features/contributionRegistry';

interface ContributionRegistryState {
  snapshot: ContributionRegistrySnapshot;
  isRefreshing: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

let refreshPromise: Promise<void> | null = null;
let eventListenerInstalled = false;

export const useContributionRegistryStore = create<ContributionRegistryState>((set) => ({
  snapshot: EMPTY_CONTRIBUTION_REGISTRY,
  isRefreshing: false,
  error: null,

  refresh: async () => {
    if (refreshPromise) {
      return refreshPromise;
    }

    refreshPromise = (async () => {
      set({ isRefreshing: true, error: null });
      try {
        const overview = await getPlatformModuleRuntime();
        set({
          snapshot: buildContributionRegistry(overview.modules),
          error: null,
        });
      } catch (error) {
        set({ error: String(error) });
      } finally {
        set({ isRefreshing: false });
        refreshPromise = null;
      }
    })();

    return refreshPromise;
  },
}));

export function initializeContributionRegistry() {
  if (!eventListenerInstalled && typeof window !== 'undefined') {
    eventListenerInstalled = true;
    window.addEventListener(PLATFORM_MODULE_RUNTIME_CHANGED_EVENT, () => {
      void useContributionRegistryStore.getState().refresh();
    });
  }
  return useContributionRegistryStore.getState().refresh();
}
