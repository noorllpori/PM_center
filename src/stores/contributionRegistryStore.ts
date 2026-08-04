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
let runtimeEventHandler: EventListener | null = null;
let subscriptionConsumerCount = 0;

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

function retainContributionRegistrySubscription() {
  subscriptionConsumerCount += 1;
  if (!runtimeEventHandler && typeof window !== 'undefined') {
    runtimeEventHandler = () => {
      void useContributionRegistryStore.getState().refresh();
    };
    window.addEventListener(PLATFORM_MODULE_RUNTIME_CHANGED_EVENT, runtimeEventHandler);
  }

  let released = false;
  return () => {
    if (released) {
      return;
    }
    released = true;
    subscriptionConsumerCount = Math.max(0, subscriptionConsumerCount - 1);
    if (subscriptionConsumerCount === 0 && runtimeEventHandler && typeof window !== 'undefined') {
      window.removeEventListener(PLATFORM_MODULE_RUNTIME_CHANGED_EVENT, runtimeEventHandler);
      runtimeEventHandler = null;
    }
  };
}

export interface ContributionRegistrySubscriptionDiagnostics {
  consumerCount: number;
  listenerInstalled: boolean;
  refreshInFlight: boolean;
}

export interface ContributionRegistrySubscriptionProbe {
  success: boolean;
  before: ContributionRegistrySubscriptionDiagnostics;
  during: ContributionRegistrySubscriptionDiagnostics;
  after: ContributionRegistrySubscriptionDiagnostics;
}

export function getContributionRegistrySubscriptionDiagnostics(): ContributionRegistrySubscriptionDiagnostics {
  return {
    consumerCount: subscriptionConsumerCount,
    listenerInstalled: runtimeEventHandler !== null,
    refreshInFlight: refreshPromise !== null,
  };
}

export function runContributionRegistrySubscriptionProbe(): ContributionRegistrySubscriptionProbe {
  const before = getContributionRegistrySubscriptionDiagnostics();
  const release = retainContributionRegistrySubscription();
  const during = getContributionRegistrySubscriptionDiagnostics();
  release();
  const after = getContributionRegistrySubscriptionDiagnostics();
  return {
    success:
      during.consumerCount === before.consumerCount + 1
      && during.listenerInstalled
      && after.consumerCount === before.consumerCount
      && after.listenerInstalled === before.listenerInstalled,
    before,
    during,
    after,
  };
}

export async function initializeContributionRegistry() {
  const release = retainContributionRegistrySubscription();
  try {
    await useContributionRegistryStore.getState().refresh();
    return release;
  } catch (error) {
    release();
    throw error;
  }
}
