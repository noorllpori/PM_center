import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { create } from 'zustand';
import type { RenderJob } from '../types/render';

interface RenderStoreState {
  jobsByProject: Record<string, RenderJob[]>;
  loadingProjects: Record<string, boolean>;
  pendingSourcesByProject: Record<string, string[]>;
  refreshProject: (projectPath: string, includeArchived?: boolean) => Promise<void>;
  queueSource: (projectPath: string, path: string) => void;
  consumePendingSources: (projectPath: string) => string[];
}

export const useRenderStore = create<RenderStoreState>((set, get) => ({
  jobsByProject: {},
  loadingProjects: {},
  pendingSourcesByProject: {},
  refreshProject: async (projectPath, includeArchived = false) => {
    set((state) => ({
      loadingProjects: { ...state.loadingProjects, [projectPath]: true },
    }));
    try {
      const jobs = await invoke<RenderJob[]>('list_render_jobs', { projectPath, includeArchived });
      set((state) => ({
        jobsByProject: { ...state.jobsByProject, [projectPath]: jobs },
      }));
    } finally {
      set((state) => ({
        loadingProjects: { ...state.loadingProjects, [projectPath]: false },
      }));
    }
  },
  queueSource: (projectPath, path) => set((state) => ({
    pendingSourcesByProject: {
      ...state.pendingSourcesByProject,
      [projectPath]: Array.from(new Set([...(state.pendingSourcesByProject[projectPath] || []), path])),
    },
  })),
  consumePendingSources: (projectPath) => {
    const sources = get().pendingSourcesByProject[projectPath] || [];
    set((state) => ({ pendingSourcesByProject: { ...state.pendingSourcesByProject, [projectPath]: [] } }));
    return sources;
  },
}));

let listenerPromise: Promise<UnlistenFn[]> | null = null;
const refreshTimers = new Map<string, number>();

function scheduleRefresh(projectPath: string) {
  const current = refreshTimers.get(projectPath);
  if (current) window.clearTimeout(current);
  refreshTimers.set(projectPath, window.setTimeout(() => {
    refreshTimers.delete(projectPath);
    void useRenderStore.getState().refreshProject(projectPath);
  }, 180));
}

export function initRenderEventListeners() {
  if (listenerPromise) return listenerPromise;
  listenerPromise = Promise.all([
    listen<{ projectPath: string }>('pm-center:render-queue-updated', ({ payload }) => {
      if (payload?.projectPath) scheduleRefresh(payload.projectPath);
    }),
    listen<{ projectPath: string }>('pm-center:render-job-progress', ({ payload }) => {
      if (payload?.projectPath) scheduleRefresh(payload.projectPath);
    }),
  ]);
  return listenerPromise;
}

export function getActiveRenderCount(jobsByProject: Record<string, RenderJob[]>) {
  return Object.values(jobsByProject).flat().filter((job) =>
    ['pending', 'starting', 'running', 'pausing', 'cancelling'].includes(job.status),
  ).length;
}
