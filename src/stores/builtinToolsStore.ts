import { load } from '@tauri-apps/plugin-store';
import { create } from 'zustand';
import {
  BUILTIN_TOOL_BY_ID,
  DEFAULT_PINNED_BUILTIN_TOOL_IDS,
  isBuiltinToolId,
  type BuiltinToolId,
} from '../features/builtinTools';

export interface BuiltinToolPreferences {
  version: 1;
  pinnedToolIds: BuiltinToolId[];
}

interface BuiltinToolsState {
  pinnedToolIds: BuiltinToolId[];
  isLoaded: boolean;
  loadPreferences: () => Promise<void>;
  togglePinned: (toolId: BuiltinToolId) => Promise<void>;
  reorderPinned: (toolId: BuiltinToolId, beforeToolId: BuiltinToolId | null) => Promise<void>;
  replacePinnedByContributionIds: (contributionIds: string[]) => Promise<void>;
}

const STORE_FILE = 'builtin-tools.json';
const STORE_KEY = 'preferences';
let storePromise: Promise<Awaited<ReturnType<typeof load>>> | null = null;
let loadPromise: Promise<void> | null = null;

function getStore() {
  if (!storePromise) {
    storePromise = load(STORE_FILE);
  }
  return storePromise;
}

function sanitizePinnedToolIds(values: unknown): BuiltinToolId[] {
  if (!Array.isArray(values)) {
    return [...DEFAULT_PINNED_BUILTIN_TOOL_IDS];
  }

  const seen = new Set<BuiltinToolId>();
  return values.flatMap((value) => {
    if (!isBuiltinToolId(value) || seen.has(value)) {
      return [];
    }

    const definition = BUILTIN_TOOL_BY_ID.get(value);
    if (!definition?.pinnable) {
      return [];
    }

    seen.add(value);
    return [value];
  });
}

async function persistPreferences(pinnedToolIds: BuiltinToolId[]) {
  const store = await getStore();
  const preferences: BuiltinToolPreferences = {
    version: 1,
    pinnedToolIds,
  };
  await store.set(STORE_KEY, preferences);
  await store.save();
}

export function getPinnedToolContributionIds(pinnedToolIds?: BuiltinToolId[]) {
  const values = pinnedToolIds ?? useBuiltinToolsStore.getState().pinnedToolIds;
  return values.flatMap((toolId) => {
    const contributionId = BUILTIN_TOOL_BY_ID.get(toolId)?.contribution.id;
    return contributionId ? [contributionId] : [];
  });
}

export function getKnownToolContributionIds() {
  return Array.from(BUILTIN_TOOL_BY_ID.values(), (definition) => definition.contribution.id);
}

function resolvePinnedContributionIds(contributionIds: string[]): BuiltinToolId[] {
  const toolIdByContribution = new Map(
    Array.from(BUILTIN_TOOL_BY_ID.values(), (definition) => [
      definition.contribution.id,
      definition.id,
    ] as const),
  );
  const seen = new Set<BuiltinToolId>();
  return contributionIds.map((contributionId) => {
    const toolId = toolIdByContribution.get(contributionId);
    const definition = toolId ? BUILTIN_TOOL_BY_ID.get(toolId) : null;
    if (!toolId || !definition?.pinnable) {
      throw new Error(`固定工具没有可用实现：${contributionId}`);
    }
    return toolId;
  }).filter((toolId) => {
    if (seen.has(toolId)) {
      return false;
    }
    seen.add(toolId);
    return true;
  });
}

export const useBuiltinToolsStore = create<BuiltinToolsState>((set, get) => ({
  pinnedToolIds: [...DEFAULT_PINNED_BUILTIN_TOOL_IDS],
  isLoaded: false,

  loadPreferences: async () => {
    if (get().isLoaded) {
      return;
    }
    if (loadPromise) {
      return loadPromise;
    }

    loadPromise = (async () => {
      try {
        const store = await getStore();
        const stored = await store.get<BuiltinToolPreferences>(STORE_KEY);
        const pinnedToolIds = stored?.version === 1
          ? sanitizePinnedToolIds(stored.pinnedToolIds)
          : [...DEFAULT_PINNED_BUILTIN_TOOL_IDS];
        set({ pinnedToolIds, isLoaded: true });

        if (!stored || stored.version !== 1) {
          await persistPreferences(pinnedToolIds);
        }
      } catch (error) {
        console.error('Failed to load builtin tool preferences:', error);
        set({ isLoaded: true });
      } finally {
        loadPromise = null;
      }
    })();

    return loadPromise;
  },

  togglePinned: async (toolId) => {
    const definition = BUILTIN_TOOL_BY_ID.get(toolId);
    if (!definition?.pinnable) {
      return;
    }

    const pinnedToolIds = get().pinnedToolIds.includes(toolId)
      ? get().pinnedToolIds.filter((id) => id !== toolId)
      : [...get().pinnedToolIds, toolId];
    set({ pinnedToolIds });

    try {
      await persistPreferences(pinnedToolIds);
    } catch (error) {
      console.error('Failed to save builtin tool preferences:', error);
    }
  },

  reorderPinned: async (toolId, beforeToolId) => {
    const current = get().pinnedToolIds;
    const fromIndex = current.indexOf(toolId);
    if (fromIndex < 0 || toolId === beforeToolId) {
      return;
    }

    const next = current.filter((id) => id !== toolId);
    const targetIndex = beforeToolId ? next.indexOf(beforeToolId) : next.length;
    next.splice(targetIndex < 0 ? next.length : targetIndex, 0, toolId);
    set({ pinnedToolIds: next });

    try {
      await persistPreferences(next);
    } catch (error) {
      console.error('Failed to save builtin tool order:', error);
    }
  },

  replacePinnedByContributionIds: async (contributionIds) => {
    const pinnedToolIds = resolvePinnedContributionIds(contributionIds);
    await persistPreferences(pinnedToolIds);
    set({ pinnedToolIds });
  },
}));
