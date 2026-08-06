import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { create } from 'zustand';
import {
  cancelAutomationRun,
  getAutomationRuntimeSnapshot,
  resolveAutomationAttention,
  retryAutomationRun,
  startAutomationRun,
} from '../api/scriptAutomation';
import type {
  AutomationAttentionAction,
  AutomationRun,
  AutomationRuntimeSnapshot,
  StartAutomationRunRequest,
} from '../types/automation';

interface AutomationState {
  snapshot: AutomationRuntimeSnapshot | null;
  selectedRunId: string | null;
  loading: boolean;
  error: string | null;
  initialize: () => Promise<void>;
  refresh: () => Promise<void>;
  selectRun: (runId: string | null) => void;
  startRun: (request: StartAutomationRunRequest) => Promise<AutomationRun>;
  cancelRun: (runId: string) => Promise<void>;
  retryRun: (runId: string) => Promise<void>;
  resolveAttention: (runId: string, action: AutomationAttentionAction) => Promise<void>;
}

let initializationPromise: Promise<void> | null = null;
let listeners: UnlistenFn[] = [];

function replaceRun(snapshot: AutomationRuntimeSnapshot | null, run: AutomationRun) {
  if (!snapshot) return snapshot;
  const recentRuns = [run, ...snapshot.recentRuns.filter((item) => item.id !== run.id)]
    .sort((left, right) => right.createdAt - left.createdAt)
    .slice(0, 200);
  return {
    ...snapshot,
    recentRuns,
    activeCount: recentRuns.filter((item) => ['queued', 'preparing', 'running', 'cancelling'].includes(item.status)).length,
    waitingPermissionCount: recentRuns.filter((item) => item.status === 'waiting-permission').length,
    attentionCount: recentRuns.filter((item) => item.status === 'attention').length,
  };
}

export const useAutomationStore = create<AutomationState>((set, get) => ({
  snapshot: null,
  selectedRunId: null,
  loading: false,
  error: null,

  initialize: async () => {
    if (initializationPromise) return initializationPromise;
    initializationPromise = (async () => {
      if (listeners.length === 0) {
        const unlistenRun = await listen<AutomationRun>('nexora:automation-run-changed', ({ payload }) => {
          set((state) => ({
            snapshot: replaceRun(state.snapshot, payload),
            selectedRunId: state.selectedRunId ?? payload.id,
          }));
        });
        const unlistenLog = await listen<{ runId: string; line: string }>('nexora:automation-log', ({ payload }) => {
          set((state) => {
            const snapshot = state.snapshot;
            if (!snapshot) return state;
            const run = snapshot.recentRuns.find((item) => item.id === payload.runId);
            if (!run || run.logs.includes(payload.line)) return state;
            return { snapshot: replaceRun(snapshot, { ...run, logs: [...run.logs, payload.line] }) };
          });
        });
        listeners = [unlistenRun, unlistenLog];
      }
      await get().refresh();
    })().finally(() => {
      initializationPromise = null;
    });
    return initializationPromise;
  },

  refresh: async () => {
    set({ loading: true, error: null });
    try {
      const snapshot = await getAutomationRuntimeSnapshot();
      set((state) => ({
        snapshot,
        selectedRunId: state.selectedRunId && snapshot.recentRuns.some((run) => run.id === state.selectedRunId)
          ? state.selectedRunId
          : snapshot.recentRuns[0]?.id ?? null,
      }));
    } catch (error) {
      set({ error: String(error) });
    } finally {
      set({ loading: false });
    }
  },

  selectRun: (selectedRunId) => set({ selectedRunId }),

  startRun: async (request) => {
    const run = await startAutomationRun(request);
    set((state) => ({
      snapshot: replaceRun(state.snapshot, run),
      selectedRunId: run.id,
      error: null,
    }));
    return run;
  },

  cancelRun: async (runId) => {
    const run = await cancelAutomationRun(runId);
    set((state) => ({ snapshot: replaceRun(state.snapshot, run), error: null }));
  },

  retryRun: async (runId) => {
    const run = await retryAutomationRun(runId);
    set((state) => ({ snapshot: replaceRun(state.snapshot, run), selectedRunId: run.id, error: null }));
  },

  resolveAttention: async (runId, action) => {
    const run = await resolveAutomationAttention(runId, action);
    set((state) => ({ snapshot: replaceRun(state.snapshot, run), error: null }));
  },
}));
