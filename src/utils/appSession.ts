import { load } from '@tauri-apps/plugin-store';
import type { WorkspaceTabType } from '../stores/workspaceTabStore';

const STORE_FILE = 'app-session.json';
const SESSION_KEY = 'appSession';
const SESSION_VERSION = 3;

export type PersistedWorkspaceTabType = Exclude<WorkspaceTabType, 'files'>;

export interface PersistedWorkspaceTab {
  type: PersistedWorkspaceTabType;
  filePath?: string;
  title?: string;
  contributionId?: string;
}

export interface PersistedWorkspaceActiveTab {
  type: WorkspaceTabType;
  filePath?: string;
  contributionId?: string;
}

export interface PersistedProjectSession {
  projectPath: string;
  title: string;
  currentPath: string | null;
  showExcludedFiles: boolean;
  tabs: PersistedWorkspaceTab[];
  activeTab: PersistedWorkspaceActiveTab;
}

export type PersistedShellActiveTab =
  | { type: 'home' }
  | { type: 'lan' }
  | { type: 'project'; projectPath: string };

export interface PersistedShellProjectTab {
  projectPath: string;
  title: string;
}

export interface PersistedStandaloneWindow {
  instanceId: string;
  type: Extract<WorkspaceTabType, 'directory' | 'image' | 'text' | 'video'>;
  filePath: string;
  projectPath?: string;
  title?: string;
}

export interface PersistedAppSession {
  version: number;
  projectTabs: PersistedShellProjectTab[];
  utilityTabs: Array<'lan'>;
  activeTab: PersistedShellActiveTab;
  projects: PersistedProjectSession[];
  standaloneWindows: PersistedStandaloneWindow[];
}

function createEmptySession(): PersistedAppSession {
  return {
    version: SESSION_VERSION,
    projectTabs: [],
    utilityTabs: [],
    activeTab: { type: 'home' },
    projects: [],
    standaloneWindows: [],
  };
}

function normalizePathKey(path?: string | null) {
  if (!path) {
    return '';
  }

  return path
    .replace(/[\\/]+/g, '/')
    .replace(/\/$/, '')
    .toLowerCase();
}

function sanitizeContributionId(value: unknown) {
  return typeof value === 'string' && /^[a-z0-9][a-z0-9.-]*$/.test(value)
    ? value
    : undefined;
}

function sanitizeWorkspaceTab(tab: unknown): PersistedWorkspaceTab | null {
  if (!tab || typeof tab !== 'object') {
    return null;
  }

  const candidate = tab as Partial<PersistedWorkspaceTab>;
  if (
    candidate.type !== 'directory' &&
    candidate.type !== 'image' &&
    candidate.type !== 'text' &&
    candidate.type !== 'video' &&
    candidate.type !== 'blend' &&
    candidate.type !== 'cache' &&
    candidate.type !== 'render' &&
    candidate.type !== 'p2p' &&
    candidate.type !== 'contribution'
  ) {
    return null;
  }

  const contributionId = sanitizeContributionId(candidate.contributionId);
  if (candidate.type === 'contribution' && !contributionId) {
    return null;
  }
  if (
    candidate.type !== 'cache' &&
    candidate.type !== 'render' &&
    candidate.type !== 'p2p' &&
    candidate.type !== 'contribution' &&
    !candidate.filePath
  ) {
    return null;
  }

  return {
    type: candidate.type,
    filePath: candidate.filePath || undefined,
    title: candidate.title || undefined,
    contributionId,
  };
}

function sanitizeActiveWorkspaceTab(tab: unknown): PersistedWorkspaceActiveTab {
  if (!tab || typeof tab !== 'object') {
    return { type: 'files' };
  }

  const candidate = tab as Partial<PersistedWorkspaceActiveTab>;
  if (
    candidate.type !== 'files' &&
    candidate.type !== 'directory' &&
    candidate.type !== 'image' &&
    candidate.type !== 'text' &&
    candidate.type !== 'video' &&
    candidate.type !== 'blend' &&
    candidate.type !== 'cache' &&
    candidate.type !== 'render' &&
    candidate.type !== 'p2p' &&
    candidate.type !== 'contribution'
  ) {
    return { type: 'files' };
  }

  const contributionId = sanitizeContributionId(candidate.contributionId);
  if (candidate.type === 'contribution' && !contributionId) {
    return { type: 'files' };
  }
  if (
    candidate.type !== 'files' &&
    candidate.type !== 'cache' &&
    candidate.type !== 'render' &&
    candidate.type !== 'p2p' &&
    candidate.type !== 'contribution' &&
    !candidate.filePath
  ) {
    return { type: 'files' };
  }

  return {
    type: candidate.type,
    filePath: candidate.filePath || undefined,
    contributionId,
  };
}

function sanitizeProjectSession(session: unknown): PersistedProjectSession | null {
  if (!session || typeof session !== 'object') {
    return null;
  }

  const candidate = session as Partial<PersistedProjectSession>;
  if (!candidate.projectPath || !candidate.title) {
    return null;
  }

  return {
    projectPath: candidate.projectPath,
    title: candidate.title,
    currentPath: typeof candidate.currentPath === 'string' ? candidate.currentPath : null,
    showExcludedFiles: Boolean(candidate.showExcludedFiles),
    tabs: Array.isArray(candidate.tabs)
      ? candidate.tabs
          .map(sanitizeWorkspaceTab)
          .filter((tab): tab is PersistedWorkspaceTab => Boolean(tab))
      : [],
    activeTab: sanitizeActiveWorkspaceTab(candidate.activeTab),
  };
}

function sanitizeStandaloneWindow(window: unknown): PersistedStandaloneWindow | null {
  if (!window || typeof window !== 'object') {
    return null;
  }

  const candidate = window as Partial<PersistedStandaloneWindow>;
  if (
    !candidate.instanceId ||
    !candidate.filePath ||
    (
      candidate.type !== 'directory' &&
      candidate.type !== 'image' &&
      candidate.type !== 'text' &&
      candidate.type !== 'video'
    )
  ) {
    return null;
  }

  return {
    instanceId: candidate.instanceId,
    type: candidate.type,
    filePath: candidate.filePath,
    projectPath: candidate.projectPath || undefined,
    title: candidate.title || undefined,
  };
}

const trackedStandaloneWindows = new Map<string, PersistedStandaloneWindow>();
const trackedStandaloneWindowListeners = new Set<() => void>();

function emitTrackedStandaloneWindowChange() {
  trackedStandaloneWindowListeners.forEach((listener) => listener());
}

export function subscribeTrackedStandaloneWindows(listener: () => void) {
  trackedStandaloneWindowListeners.add(listener);
  return () => {
    trackedStandaloneWindowListeners.delete(listener);
  };
}

export function trackStandaloneWindow(window: PersistedStandaloneWindow) {
  trackedStandaloneWindows.set(window.instanceId, window);
  emitTrackedStandaloneWindowChange();
}

export function untrackStandaloneWindow(instanceId: string) {
  if (trackedStandaloneWindows.delete(instanceId)) {
    emitTrackedStandaloneWindowChange();
  }
}

export function getTrackedStandaloneWindows(): PersistedStandaloneWindow[] {
  return Array.from(trackedStandaloneWindows.values());
}

export async function loadPersistedAppSession(): Promise<PersistedAppSession | null> {
  try {
    const store = await load(STORE_FILE);
    const persisted = await store.get<PersistedAppSession>(SESSION_KEY);
    if (!persisted || typeof persisted !== 'object') {
      return null;
    }

    const projectTabs = Array.isArray(persisted.projectTabs)
      ? persisted.projectTabs.filter(
          (tab): tab is PersistedShellProjectTab =>
            Boolean(tab?.projectPath) && Boolean(tab?.title),
        )
      : [];

    const projects = Array.isArray(persisted.projects)
      ? persisted.projects
          .map(sanitizeProjectSession)
          .filter((session): session is PersistedProjectSession => Boolean(session))
      : [];

    const utilityTabs = Array.isArray(persisted.utilityTabs)
      ? persisted.utilityTabs.filter((tab): tab is 'lan' => tab === 'lan')
      : [];

    const standaloneWindows = Array.isArray(persisted.standaloneWindows)
      ? persisted.standaloneWindows
          .map(sanitizeStandaloneWindow)
          .filter((window): window is PersistedStandaloneWindow => Boolean(window))
      : [];

    const activeTab = persisted.activeTab?.type === 'project' && persisted.activeTab.projectPath
      ? { type: 'project' as const, projectPath: persisted.activeTab.projectPath }
      : persisted.activeTab?.type === 'lan'
        ? { type: 'lan' as const }
        : { type: 'home' as const };

    return {
      version: SESSION_VERSION,
      projectTabs,
      utilityTabs,
      activeTab,
      projects,
      standaloneWindows,
    };
  } catch (error) {
    console.error('Failed to load persisted app session:', error);
    return null;
  }
}

export async function savePersistedAppSession(session: PersistedAppSession) {
  const store = await load(STORE_FILE);
  await store.set(SESSION_KEY, {
    ...session,
    version: SESSION_VERSION,
  });
  await store.save();
}

export async function clearPersistedAppSession() {
  const store = await load(STORE_FILE);
  await store.delete(SESSION_KEY);
  await store.save();
}

export function dedupeStandaloneWindows(
  windows: PersistedStandaloneWindow[],
): PersistedStandaloneWindow[] {
  const seen = new Set<string>();
  const deduped: PersistedStandaloneWindow[] = [];

  for (const window of windows) {
    const key = [
      window.instanceId,
      window.type,
      normalizePathKey(window.filePath),
      normalizePathKey(window.projectPath),
    ].join('::');

    if (seen.has(key)) {
      continue;
    }

    seen.add(key);
    deduped.push(window);
  }

  return deduped;
}

export function createDefaultPersistedAppSession(): PersistedAppSession {
  return createEmptySession();
}
