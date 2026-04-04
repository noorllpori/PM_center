import { create } from 'zustand';
import { getPluginDirs, listPlugins, refreshPlugins, setPluginEnabled } from '../api/plugins';
import type { PluginDescriptor, PluginDirectories } from '../types/plugin';

interface PluginProjectState {
  isLoading: boolean;
  descriptors: PluginDescriptor[];
  lastLoadedAt: number | null;
}

interface PluginState {
  byProject: Record<string, PluginProjectState>;
  directories: Record<string, PluginDirectories | undefined>;
  loadPlugins: (projectPath?: string | null) => Promise<PluginDescriptor[]>;
  refreshProjectPlugins: (projectPath?: string | null) => Promise<PluginDescriptor[]>;
  loadPluginDirs: (projectPath?: string | null) => Promise<PluginDirectories>;
  togglePlugin: (projectPath: string | null | undefined, pluginKey: string, enabled: boolean) => Promise<void>;
}

const EMPTY_PROJECT_STATE: PluginProjectState = {
  isLoading: false,
  descriptors: [],
  lastLoadedAt: null,
};

function projectKey(projectPath?: string | null) {
  return projectPath || '__global__';
}

export const usePluginStore = create<PluginState>((set, get) => ({
  byProject: {},
  directories: {},

  loadPlugins: async (projectPath) => {
    const key = projectKey(projectPath);
    const existing = get().byProject[key];
    if (existing?.lastLoadedAt) {
      return existing.descriptors;
    }

    return get().refreshProjectPlugins(projectPath);
  },

  refreshProjectPlugins: async (projectPath) => {
    const key = projectKey(projectPath);
    set((state) => ({
      byProject: {
        ...state.byProject,
        [key]: {
          ...(state.byProject[key] || EMPTY_PROJECT_STATE),
          isLoading: true,
        },
      },
    }));

    try {
      const descriptors = await refreshPlugins(projectPath);
      set((state) => ({
        byProject: {
          ...state.byProject,
          [key]: {
            isLoading: false,
            descriptors,
            lastLoadedAt: Date.now(),
          },
        },
      }));
      return descriptors;
    } catch (error) {
      set((state) => ({
        byProject: {
          ...state.byProject,
          [key]: {
            ...(state.byProject[key] || EMPTY_PROJECT_STATE),
            isLoading: false,
          },
        },
      }));
      throw error;
    }
  },

  loadPluginDirs: async (projectPath) => {
    const key = projectKey(projectPath);
    const directories = await getPluginDirs(projectPath);
    set((state) => ({
      directories: {
        ...state.directories,
        [key]: directories,
      },
    }));
    return directories;
  },

  togglePlugin: async (projectPath, pluginKey, enabled) => {
    await setPluginEnabled(pluginKey, enabled);
    const key = projectKey(projectPath);
    const descriptors = await listPlugins(projectPath);
    const directories = await getPluginDirs(projectPath);
    set((state) => ({
      byProject: {
        ...state.byProject,
        [key]: {
          isLoading: false,
          descriptors,
          lastLoadedAt: Date.now(),
        },
      },
      directories: {
        ...state.directories,
        [key]: directories,
      },
    }));
  },
}));
