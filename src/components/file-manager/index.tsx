import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ShieldCheck, X } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { PythonEnvManager } from '../PythonEnvManager';
import { SettingsPanel } from '../SettingsPanel';
import { RecoverySettingsPanel } from '../settings/RecoverySettingsPanel';
import { TaskPanel } from '../TaskPanel';
import { openStandaloneDirectoryViewer } from './openStandaloneDirectoryViewer';
import { openStandaloneImageViewer } from '../image-viewer/openStandaloneImageViewer';
import { LauncherButton } from '../Launcher';
import { BlenderFileParserDialog } from '../tools/BlenderFileParserDialog';
import { ScriptDeveloperWorkbench } from '../automation/ScriptDeveloperWorkbench';
import { ScriptSurfaceFrame } from '../automation/ScriptSurfaceFrame';
import type { ScriptSurfaceTool } from '../BuiltinToolsCenter';
import { openStandaloneTextEditor } from '../text-editor/openStandaloneTextEditor';
import { openStandaloneVideoPlayer } from '../video-player/openStandaloneVideoPlayer';
import { Toolbar, TOOLBAR_SEARCH_FOCUS_EVENT } from './Toolbar';
import { CLOSE_MDT_OVERVIEW_EVENT, ProjectWorkspace } from './ProjectWorkspace';
import { ProjectSessionProvider } from './ProjectSessionProvider';
import { ShellTabBar } from '../shell/ShellTabBar';
import { ContributedShellSurface } from '../shell/ContributedShellSurface';
import { ProfileHomeSurface } from '../shell/ProfileHomeSurface';
import { ProfileNavigationBar } from '../shell/ProfileNavigationBar';
import { DevelopmentReloadControl } from '../shell/DevelopmentReloadControl';
import { PinnedToolsToolbar } from './PinnedToolsToolbar';
import { OPEN_RECOVERY_SETTINGS_EVENT } from '../../features/recoverySettings';
import {
  getProfileHomeScriptSurfaceTarget,
  resolveProfileNavigation,
} from '../../features/profileLayout';
import { Dialog } from '../Dialog';
import { ProjectLocationDialog } from './ProjectLocationDialog';
import { createProjectStore, type ProjectStoreApi } from '../../stores/projectStore';
import { useTaskStore } from '../../stores/taskStore';
import { createWorkspaceTabStore, type WorkspaceTabStoreApi } from '../../stores/workspaceTabStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useUiStore } from '../../stores/uiStore';
import { useShellTabStore, normalizeProjectPath } from '../../stores/shellTabStore';
import {
  getPinnedToolContributionIds,
  useBuiltinToolsStore,
} from '../../stores/builtinToolsStore';
import { useLanCollaborationStore } from '../../stores/lanCollaborationStore';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import {
  BUILTIN_TOOL_BY_ID,
  type BuiltinToolDialogId,
  type BuiltinToolId,
} from '../../features/builtinTools';
import {
  SHELL_TAB_CONTRIBUTIONS,
  SHELL_TAB_CONTRIBUTION_BY_ID,
  TOOL_CONTRIBUTIONS,
  WORKSPACE_TAB_CONTRIBUTION_BY_ID,
  WORKSPACE_TAB_CONTRIBUTION_BY_TYPE,
  WORKSPACE_TAB_CONTRIBUTIONS,
  getContributionUnavailableReason,
  getShellTabContributionUnavailableReason,
  getWorkspaceTabContributionUnavailableReason,
  isContributionAvailable,
  isShellTabContributionAvailable,
  isWorkspaceTabContributionAvailable,
} from '../../features/contributionRegistry';
import {
  initializeContributionRegistry,
  useContributionRegistryStore,
} from '../../stores/contributionRegistryStore';
import {
  createDefaultPersistedAppSession,
  dedupeStandaloneWindows,
  getAppSessionProfileCompatibility,
  getTrackedStandaloneWindows,
  loadPersistedAppSession,
  savePersistedAppSession,
  subscribeTrackedStandaloneWindows,
  type PersistedAppSession,
  type PersistedProjectSession,
  type PersistedStandaloneWindow,
  type PersistedWorkspaceActiveTab,
  type PersistedWorkspaceTab,
} from '../../utils/appSession';
import type { PluginControlMessage, PluginInteractionResponse } from '../../types/plugin';
import type { Task } from '../../types/task';
import {
  STANDALONE_RETURN_TO_WORKSPACE_EVENT,
  type StandaloneReturnToWorkspacePayload,
} from '../workspace/standaloneWindowReturn';
import {
  findProjectLocationCandidates,
  inspectProjectLocation,
  type ProjectLocationCandidate,
  type ProjectLocationReport,
} from '../../api/projects';
import { emitAutomationEvent } from '../../api/scriptAutomation';

interface ProjectSession {
  projectStore: ProjectStoreApi;
  workspaceTabStore: WorkspaceTabStoreApi;
}

interface ProjectSessionSubscriptions {
  unsubscribeProject: () => void;
  unsubscribeWorkspace: () => void;
}

interface PluginConfirmDialogState {
  isOpen: boolean;
  task: Task | null;
  requestId: string;
  title: string;
  message: string;
  confirmText: string;
  cancelText: string;
  items: string[];
  data?: unknown;
}

interface OpenProjectOptions {
  skipRecentTracking?: boolean;
}

interface PendingProjectOpen {
  path: string;
  options?: OpenProjectOptions;
  report: ProjectLocationReport;
}

const SESSION_PERSIST_DEBOUNCE_MS = 180;

function getProjectNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || 'Project';
}

function getFileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function getPersistedWorkspaceTabKey(tab: PersistedWorkspaceTab | PersistedWorkspaceActiveTab) {
  if (tab.contributionId) {
    return `contribution:${tab.contributionId}`;
  }
  return tab.type === 'files' || tab.type === 'cache' || tab.type === 'render' || tab.type === 'p2p'
    ? tab.type
    : `${tab.type}:${tab.filePath || ''}`;
}

function serializeWorkspaceSession(
  workspaceTabStore: WorkspaceTabStoreApi,
): Pick<PersistedProjectSession, 'tabs' | 'activeTab'> {
  const state = workspaceTabStore.getState();
  const tabs = state.tabs.flatMap<PersistedWorkspaceTab>((tab) => {
    if (tab.type === 'files') {
      return [];
    }

    if (tab.contributionId) {
      return [{
        type: tab.type as PersistedWorkspaceTab['type'],
        title: tab.title,
        contributionId: tab.contributionId,
      }];
    }

    if (!tab.filePath) {
      return [];
    }

    return [{
      type: tab.type,
      filePath: tab.filePath,
      title: tab.title,
    }];
  });

  const activeTab = state.tabs.find((tab) => tab.id === state.activeTabId) ?? state.tabs[0];
  const activePersistedTab: PersistedWorkspaceActiveTab =
    activeTab?.contributionId
      ? { type: activeTab.type, contributionId: activeTab.contributionId }
      : activeTab?.type && activeTab.type !== 'files' && activeTab.filePath
        ? { type: activeTab.type, filePath: activeTab.filePath }
        : { type: 'files' };

  return {
    tabs,
    activeTab: activePersistedTab,
  };
}

async function restoreWorkspaceSession(
  workspaceTabStore: WorkspaceTabStoreApi,
  session: PersistedProjectSession,
) {
  workspaceTabStore.getState().resetTabs();

  const restoredTabIds = new Map<string, string>();
  const contributionSnapshot = useContributionRegistryStore.getState().snapshot;

  for (const tab of session.tabs) {
    const contribution = tab.contributionId
      ? WORKSPACE_TAB_CONTRIBUTION_BY_ID.get(tab.contributionId)
      : WORKSPACE_TAB_CONTRIBUTION_BY_TYPE.get(tab.type);
    if (tab.contributionId && !contribution) {
      continue;
    }
    if (contribution) {
      if (!isWorkspaceTabContributionAvailable(contributionSnapshot, contribution)) {
        continue;
      }
      const tabId = workspaceTabStore
        .getState()
        .openWorkspaceContributionTab(contribution.id);
      if (tabId) {
        restoredTabIds.set(getPersistedWorkspaceTabKey({
          ...tab,
          contributionId: contribution.id,
        }), tabId);
        restoredTabIds.set(getPersistedWorkspaceTabKey(tab), tabId);
      }
      continue;
    }

    if (tab.type === 'directory') {
      if (!tab.filePath) {
        continue;
      }

      const tabId = workspaceTabStore.getState().openDirectoryInTab(tab.filePath);
      restoredTabIds.set(getPersistedWorkspaceTabKey(tab), tabId);
      continue;
    }

    if (!tab.filePath) {
      continue;
    }

    const tabId = await workspaceTabStore.getState().openFileInTab(tab.filePath);
    if (tabId) {
      restoredTabIds.set(getPersistedWorkspaceTabKey(tab), tabId);
    }
  }

  if (session.activeTab.type === 'files') {
    workspaceTabStore.getState().activateTab('files');
    return;
  }

  const activeTabId = restoredTabIds.get(getPersistedWorkspaceTabKey(session.activeTab));
  if (activeTabId) {
    workspaceTabStore.getState().activateTab(activeTabId);
  }
}

async function restoreStandaloneWindow(window: PersistedStandaloneWindow) {
  if (window.type === 'directory') {
    await openStandaloneDirectoryViewer({
      directoryPath: window.filePath,
      title: window.title,
      projectPath: window.projectPath,
      projectName: window.projectPath ? getProjectNameFromPath(window.projectPath) : undefined,
      focus: false,
    });
    return;
  }

  if (window.type === 'image') {
    await openStandaloneImageViewer({
      filePath: window.filePath,
      title: window.title,
      projectPath: window.projectPath,
      focus: false,
    });
    return;
  }

  if (window.type === 'video') {
    await openStandaloneVideoPlayer({
      filePath: window.filePath,
      title: window.title,
      projectPath: window.projectPath,
      focus: false,
    });
    return;
  }

  await openStandaloneTextEditor({
    filePath: window.filePath,
    title: window.title,
    projectPath: window.projectPath,
    focus: false,
  });
}

export function FileManager() {
  const loadSettings = useSettingsStore((state) => state.loadSettings);
  const recentProjects = useSettingsStore((state) => state.recentProjects);
  const projectsRootDir = useSettingsStore((state) => state.projectsRootDir);
  const autoOpenLastProject = useSettingsStore((state) => state.autoOpenLastProject);
  const addRecentProject = useSettingsStore((state) => state.addRecentProject);
  const loadBuiltinToolsPreferences = useBuiltinToolsStore((state) => state.loadPreferences);
  const initializeWorkspaceProfiles = useWorkspaceProfileStore((state) => state.initialize);
  const activeWorkspaceProfile = useWorkspaceProfileStore(
    (state) => state.snapshot?.currentProfile ?? null,
  );
  const activeWorkspaceProfileId = activeWorkspaceProfile?.id ?? null;
  const activeWorkspaceProfileRevision = activeWorkspaceProfile?.revision ?? null;
  const contributionSnapshot = useContributionRegistryStore((state) => state.snapshot);
  const showToast = useUiStore((state) => state.showToast);
  const toast = useUiStore((state) => state.toast);
  const hideToast = useUiStore((state) => state.hideToast);
  const addTask = useTaskStore((state) => state.addTask);
  const tabs = useShellTabStore((state) => state.tabs);
  const activeTabId = useShellTabStore((state) => state.activeTabId);
  const openProjectTab = useShellTabStore((state) => state.openProjectTab);
  const activateTab = useShellTabStore((state) => state.activateTab);
  const closeTab = useShellTabStore((state) => state.closeTab);
  const reorderTabs = useShellTabStore((state) => state.reorderTabs);
  const activeShellTab = useMemo(
    () => tabs.find((tab) => tab.id === activeTabId) ?? tabs[0],
    [activeTabId, tabs],
  );
  const lanShellAvailable = isShellTabContributionAvailable(
    contributionSnapshot,
    SHELL_TAB_CONTRIBUTIONS.lan,
  );
  const projectShellAvailable = isShellTabContributionAvailable(
    contributionSnapshot,
    SHELL_TAB_CONTRIBUTIONS.project,
  );
  const profileNavigationItems = useMemo(
    () => resolveProfileNavigation(activeWorkspaceProfile, contributionSnapshot),
    [activeWorkspaceProfile, contributionSnapshot],
  );
  const profileNavigationKind = activeWorkspaceProfile?.shellLayout?.navigationKind ?? 'top-bar';
  const contributionShellTabs = useMemo(
    () => tabs.filter((tab) => tab.type !== 'project' && Boolean(tab.contributionId)),
    [tabs],
  );
  const isContributionShellActive = activeShellTab?.type !== 'project'
    && Boolean(activeShellTab?.contributionId);

  const [isPythonEnvOpen, setIsPythonEnvOpen] = useState(false);
  const [isTaskCenterOpen, setIsTaskCenterOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isRecoverySettingsOpen, setIsRecoverySettingsOpen] = useState(false);
  const [isBlenderFileParserOpen, setIsBlenderFileParserOpen] = useState(false);
  const [isScriptDeveloperWorkbenchOpen, setIsScriptDeveloperWorkbenchOpen] = useState(false);
  const [activeScriptSurface, setActiveScriptSurface] = useState<ScriptSurfaceTool | null>(null);
  const [blenderParserInitialFilePath, setBlenderParserInitialFilePath] = useState<string | null>(null);
  const [isSettingsLoaded, setIsSettingsLoaded] = useState(false);
  const [pluginConfirmDialog, setPluginConfirmDialog] = useState<PluginConfirmDialogState>({
    isOpen: false,
    task: null,
    requestId: '',
    title: '插件确认',
    message: '',
    confirmText: '确认',
    cancelText: '取消',
    items: [],
    data: undefined,
  });
  const [pendingProjectOpen, setPendingProjectOpen] = useState<PendingProjectOpen | null>(null);
  const [projectLocationCandidates, setProjectLocationCandidates] = useState<ProjectLocationCandidate[]>([]);
  const [isSearchingProjectLocation, setIsSearchingProjectLocation] = useState(false);
  const [projectLocationSearchError, setProjectLocationSearchError] = useState<string | null>(null);
  const [hasSearchedProjectLocation, setHasSearchedProjectLocation] = useState(false);
  const [isOpeningResolvedProject, setIsOpeningResolvedProject] = useState(false);
  const sessionsRef = useRef<Map<string, ProjectSession>>(new Map());
  const sessionSubscriptionsRef = useRef<Map<string, ProjectSessionSubscriptions>>(new Map());
  const projectReleasePromisesRef = useRef<Map<string, Promise<void>>>(new Map());
  const sessionPersistTimerRef = useRef<number | null>(null);
  const hasHandledStartupProjectRef = useRef(false);
  const isRestoringSessionRef = useRef(false);
  const isSessionPersistenceReadyRef = useRef(false);
  const suspendedProjectSessionRef = useRef<PersistedAppSession | null>(null);
  const projectShellAvailabilityRef = useRef<boolean | null>(null);
  const suppressSessionPersistenceRef = useRef(false);
  const unavailableWorkspaceContributionIds = useMemo(
    () => Object.values(WORKSPACE_TAB_CONTRIBUTIONS)
      .filter((definition) => !isWorkspaceTabContributionAvailable(contributionSnapshot, definition))
      .map((definition) => definition.id),
    [contributionSnapshot],
  );
  const pythonToolAvailable = isContributionAvailable(
    contributionSnapshot,
    TOOL_CONTRIBUTIONS.pythonEnvironments,
  );
  const taskToolAvailable = isContributionAvailable(
    contributionSnapshot,
    TOOL_CONTRIBUTIONS.taskCenter,
  );
  const mdtToolAvailable = isContributionAvailable(
    contributionSnapshot,
    TOOL_CONTRIBUTIONS.mdtOverview,
  );
  const settingsToolAvailable = isContributionAvailable(
    contributionSnapshot,
    TOOL_CONTRIBUTIONS.settings,
  );
  const blenderParserToolAvailable = isContributionAvailable(
    contributionSnapshot,
    TOOL_CONTRIBUTIONS.blenderFileParser,
  );
  const scriptAutomationToolAvailable = isContributionAvailable(
    contributionSnapshot,
    TOOL_CONTRIBUTIONS.scriptAutomation,
  );

  const openProfileNavigation = useCallback((contributionId: string) => {
    const definition = SHELL_TAB_CONTRIBUTION_BY_ID.get(contributionId);
    if (!definition) return;
    const unavailableReason = getShellTabContributionUnavailableReason(
      useContributionRegistryStore.getState().snapshot,
      definition,
    );
    if (unavailableReason) {
      showToast({
        title: `${definition.title}不可用`,
        message: unavailableReason,
        tone: 'warning',
      });
      return;
    }
    useShellTabStore.getState().openShellContributionTab(contributionId);
  }, [showToast]);

  const openProfileHome = useCallback(() => {
    const shellState = useShellTabStore.getState();
    const homeTab = shellState.tabs.find((tab) => tab.type === 'home');
    if (homeTab) {
      shellState.activateTab(homeTab.id);
    }
  }, []);

  const openScriptSurface = useCallback((surface: ScriptSurfaceTool) => {
    const homeTarget = getProfileHomeScriptSurfaceTarget(activeWorkspaceProfile);
    if (
      homeTarget
      && homeTarget.surfaceId === surface.surfaceId
      && (!homeTarget.componentId || homeTarget.componentId === surface.componentId)
    ) {
      setActiveScriptSurface(null);
      openProfileHome();
      return;
    }
    setActiveScriptSurface(surface);
  }, [activeWorkspaceProfile, openProfileHome]);

  useEffect(() => {
    if (!activeScriptSurface) return;
    const homeTarget = getProfileHomeScriptSurfaceTarget(activeWorkspaceProfile);
    if (
      homeTarget?.surfaceId === activeScriptSurface.surfaceId
      && (!homeTarget.componentId || homeTarget.componentId === activeScriptSurface.componentId)
    ) {
      setActiveScriptSurface(null);
    }
  }, [activeScriptSurface, activeWorkspaceProfile]);

  useEffect(() => {
    const openRecoverySettings = () => setIsRecoverySettingsOpen(true);
    window.addEventListener(OPEN_RECOVERY_SETTINGS_EVENT, openRecoverySettings);
    return () => window.removeEventListener(OPEN_RECOVERY_SETTINGS_EVENT, openRecoverySettings);
  }, []);

  useEffect(() => {
    if (!settingsToolAvailable) setIsSettingsOpen(false);
    if (!blenderParserToolAvailable) setIsBlenderFileParserOpen(false);
    if (!scriptAutomationToolAvailable) {
      setIsScriptDeveloperWorkbenchOpen(false);
      setActiveScriptSurface(null);
    }
  }, [blenderParserToolAvailable, scriptAutomationToolAvailable, settingsToolAvailable]);

  useEffect(() => {
    let isActive = true;
    let releaseContributionRegistry: (() => void) | null = null;

    const initializeSettings = async () => {
      await Promise.all([loadSettings(), loadBuiltinToolsPreferences()]);
      const legacyPinnedTools = getPinnedToolContributionIds();
      await initializeWorkspaceProfiles(legacyPinnedTools);
      const releaseRegistry = await initializeContributionRegistry();
      if (!isActive) {
        releaseRegistry();
      } else {
        releaseContributionRegistry = releaseRegistry;
      }
      if (isActive) {
        setIsSettingsLoaded(true);
      }
    };

    void initializeSettings();

    return () => {
      isActive = false;
      releaseContributionRegistry?.();
    };
  }, [initializeWorkspaceProfiles, loadBuiltinToolsPreferences, loadSettings]);

  useEffect(() => {
    sessionsRef.current.forEach((session) => {
      session.workspaceTabStore
        .getState()
        .closeContributionTabs(unavailableWorkspaceContributionIds);
    });
    if (!lanShellAvailable) {
      useShellTabStore
        .getState()
        .closeContributionTabs([SHELL_TAB_CONTRIBUTIONS.lan.id]);
    }
    if (!pythonToolAvailable) {
      setIsPythonEnvOpen(false);
    }
    if (!taskToolAvailable) {
      setIsTaskCenterOpen(false);
    }
    if (!mdtToolAvailable) {
      window.dispatchEvent(new Event(CLOSE_MDT_OVERVIEW_EVENT));
    }
  }, [
    lanShellAvailable,
    mdtToolAvailable,
    pythonToolAvailable,
    taskToolAvailable,
    unavailableWorkspaceContributionIds,
  ]);

  useEffect(() => {
    if (!toast.isOpen) {
      return;
    }

    const timeout = window.setTimeout(() => {
      hideToast();
    }, toast.tone === 'error' ? 6000 : 3500);

    return () => window.clearTimeout(timeout);
  }, [hideToast, toast.isOpen, toast.tone]);

  const activeProjectSession = projectShellAvailable
    && activeShellTab?.type === 'project'
    && activeShellTab.projectPath
    ? sessionsRef.current.get(normalizeProjectPath(activeShellTab.projectPath)) ?? null
    : null;

  const createPersistedAppSessionSnapshot = useCallback((): PersistedAppSession => {
    const shellState = useShellTabStore.getState();
    const projectTabs = shellState.tabs
      .filter((tab) => tab.type === 'project' && tab.projectPath)
      .map((tab) => ({
        projectPath: tab.projectPath!,
        title: tab.title,
      }));
    const utilityTabs = shellState.tabs.flatMap<'lan'>((tab) => tab.type === 'lan' ? ['lan'] : []);

    const projects = projectTabs.flatMap<PersistedProjectSession>((tab) => {
      const session = sessionsRef.current.get(normalizeProjectPath(tab.projectPath));
      if (!session) {
        return [];
      }

      const projectState = session.projectStore.getState();
      if (!projectState.projectPath || !projectState.isInitialized) {
        return [];
      }

      const workspaceSession = serializeWorkspaceSession(session.workspaceTabStore);

      return [{
        projectPath: projectState.projectPath,
        title: tab.title,
        currentPath: projectState.currentPath,
        showExcludedFiles: projectState.showExcludedFiles,
        tabs: workspaceSession.tabs,
        activeTab: workspaceSession.activeTab,
      }];
    });

    const activeTab = shellState.tabs.find((tab) => tab.id === shellState.activeTabId);
    return {
      ...createDefaultPersistedAppSession(),
      profile: (() => {
        const profile = useWorkspaceProfileStore.getState().snapshot?.currentProfile;
        return profile ? { id: profile.id, revision: profile.revision ?? 1 } : null;
      })(),
      projectTabs,
      utilityTabs,
      activeTab:
        activeTab?.type === 'project' && activeTab.projectPath
          ? { type: 'project', projectPath: activeTab.projectPath }
          : activeTab?.type === 'lan'
            ? { type: 'lan' }
          : { type: 'home' },
      projects,
      standaloneWindows: dedupeStandaloneWindows(getTrackedStandaloneWindows()),
    };
  }, []);

  const persistAppSession = useCallback(async () => {
    if (
      !isSessionPersistenceReadyRef.current
      || isRestoringSessionRef.current
      || suppressSessionPersistenceRef.current
    ) {
      return;
    }

    await savePersistedAppSession(createPersistedAppSessionSnapshot());
  }, [createPersistedAppSessionSnapshot]);

  const schedulePersistAppSession = useCallback(() => {
    if (
      !isSessionPersistenceReadyRef.current
      || isRestoringSessionRef.current
      || suppressSessionPersistenceRef.current
    ) {
      return;
    }

    if (sessionPersistTimerRef.current !== null) {
      window.clearTimeout(sessionPersistTimerRef.current);
    }

    sessionPersistTimerRef.current = window.setTimeout(() => {
      sessionPersistTimerRef.current = null;
      void persistAppSession();
    }, SESSION_PERSIST_DEBOUNCE_MS);
  }, [persistAppSession]);

  const registerSessionPersistence = useCallback((projectPath: string, session: ProjectSession) => {
    const normalizedPath = normalizeProjectPath(projectPath);
    if (sessionSubscriptionsRef.current.has(normalizedPath)) {
      return;
    }

    const unsubscribeProject = session.projectStore.subscribe((state, previous) => {
      if (
        state.currentPath !== previous.currentPath ||
        state.showExcludedFiles !== previous.showExcludedFiles ||
        state.isInitialized !== previous.isInitialized
      ) {
        schedulePersistAppSession();
      }
    });

    const unsubscribeWorkspace = session.workspaceTabStore.subscribe((state, previous) => {
      if (state.activeTabId !== previous.activeTabId || state.tabs !== previous.tabs) {
        schedulePersistAppSession();
      }
    });

    sessionSubscriptionsRef.current.set(normalizedPath, {
      unsubscribeProject,
      unsubscribeWorkspace,
    });
  }, [schedulePersistAppSession]);

  const unregisterSessionPersistence = useCallback((projectPath: string) => {
    const normalizedPath = normalizeProjectPath(projectPath);
    const subscriptions = sessionSubscriptionsRef.current.get(normalizedPath);
    if (!subscriptions) {
      return;
    }

    subscriptions.unsubscribeProject();
    subscriptions.unsubscribeWorkspace();
    sessionSubscriptionsRef.current.delete(normalizedPath);
  }, []);

  const releaseProjectSession = useCallback((projectPath: string) => {
    const normalizedPath = normalizeProjectPath(projectPath);
    unregisterSessionPersistence(projectPath);
    sessionsRef.current.delete(normalizedPath);

    const existingRelease = projectReleasePromisesRef.current.get(normalizedPath);
    if (existingRelease) {
      return existingRelease;
    }

    void emitAutomationEvent(
      'project.closed',
      { projectPath, closedAt: Date.now() },
      projectPath,
      `project.closed:${normalizedPath}:${Date.now()}`,
    ).catch(() => {});

    const release = invoke('release_project_resources', { projectPath })
      .catch((error) => {
        // Closing UI state must not depend on best-effort native cleanup.
        console.warn('Failed to release project resources:', projectPath, error);
      })
      .then(() => undefined)
      .finally(() => {
        if (projectReleasePromisesRef.current.get(normalizedPath) === release) {
          projectReleasePromisesRef.current.delete(normalizedPath);
        }
      });
    projectReleasePromisesRef.current.set(normalizedPath, release);
    return release;
  }, [unregisterSessionPersistence]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const hasCommandModifier = event.ctrlKey || event.metaKey;
      if (!hasCommandModifier || event.shiftKey || event.altKey || event.key.toLowerCase() !== 'f') {
        return;
      }

      event.preventDefault();

      if (!activeProjectSession) {
        return;
      }

      window.dispatchEvent(new Event(TOOLBAR_SEARCH_FOCUS_EVENT));
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [activeProjectSession]);

  useEffect(() => {
    const handleTaskShortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'b') {
        event.preventDefault();
        const snapshot = useContributionRegistryStore.getState().snapshot;
        const unavailableReason = getContributionUnavailableReason(
          snapshot,
          TOOL_CONTRIBUTIONS.taskCenter,
        );
        if (unavailableReason) {
          showToast({
            title: '任务中心不可用',
            message: unavailableReason,
            tone: 'warning',
          });
          return;
        }
        setIsTaskCenterOpen(true);
      }
    };

    window.addEventListener('keydown', handleTaskShortcut);
    return () => window.removeEventListener('keydown', handleTaskShortcut);
  }, [showToast]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;

    void listen<{ conversationId: string; messageId?: string | null; transferId?: string | null }>(
      'pm-center:open-lan-conversation',
      (event) => {
        const snapshot = useContributionRegistryStore.getState().snapshot;
        if (!isShellTabContributionAvailable(snapshot, SHELL_TAB_CONTRIBUTIONS.lan)) {
          return;
        }
        useShellTabStore.getState().openLanTab();
        useLanCollaborationStore.getState().requestConversationNavigation(
          event.payload.conversationId,
          event.payload.messageId,
          event.payload.transferId,
        );
      },
    ).then((cleanup) => {
      if (disposed) {
        cleanup();
      } else {
        unlisten = cleanup;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (activeShellTab?.type !== 'project' || !activeShellTab.projectPath) {
      return;
    }

    const session = sessionsRef.current.get(normalizeProjectPath(activeShellTab.projectPath));
    if (!session) {
      return;
    }

    void session.projectStore.getState().activateProject();
  }, [activeShellTab]);

  const ensureProjectSession = useCallback(async (path: string) => {
    const normalizedPath = normalizeProjectPath(path);
    let session = sessionsRef.current.get(normalizedPath);

    if (!session) {
      const pendingRelease = projectReleasePromisesRef.current.get(normalizedPath);
      if (pendingRelease) {
        await pendingRelease;
        session = sessionsRef.current.get(normalizedPath);
      }
    }

    if (!session) {
      session = {
        projectStore: createProjectStore(),
        workspaceTabStore: createWorkspaceTabStore(),
      };
      await session.projectStore.getState().setProject(path);
      sessionsRef.current.set(normalizedPath, session);
      registerSessionPersistence(path, session);
    }

    return session;
  }, [registerSessionPersistence]);

  const openProjectSession = useCallback(async (
    path: string,
    options?: OpenProjectOptions,
  ) => {
    const unavailableReason = getShellTabContributionUnavailableReason(
      useContributionRegistryStore.getState().snapshot,
      SHELL_TAB_CONTRIBUTIONS.project,
    );
    if (unavailableReason) {
      throw new Error(`项目管理器不可用：${unavailableReason}`);
    }

    const normalizedPath = normalizeProjectPath(path);
    const wasAlreadyOpen = sessionsRef.current.has(normalizedPath);
    const session = await ensureProjectSession(path);
    const unavailableAfterOpen = getShellTabContributionUnavailableReason(
      useContributionRegistryStore.getState().snapshot,
      SHELL_TAB_CONTRIBUTIONS.project,
    );
    if (unavailableAfterOpen) {
      void releaseProjectSession(path);
      throw new Error(`项目管理器不可用：${unavailableAfterOpen}`);
    }
    const projectName = session.projectStore.getState().projectName || getProjectNameFromPath(path);
    openProjectTab(path, projectName);
    if (!options?.skipRecentTracking) {
      await addRecentProject(path, projectName);
    }
    if (!wasAlreadyOpen) {
      void emitAutomationEvent(
        'project.opened',
        { projectPath: path, projectName, openedAt: Date.now() },
        path,
        `project.opened:${normalizedPath}:${Date.now()}`,
      ).catch(() => {});
    }
    return session;
  }, [addRecentProject, ensureProjectSession, openProjectTab, releaseProjectSession]);

  const closeProjectLocationDialog = useCallback(() => {
    if (isOpeningResolvedProject) {
      return;
    }
    setPendingProjectOpen(null);
    setProjectLocationCandidates([]);
    setProjectLocationSearchError(null);
    setHasSearchedProjectLocation(false);
  }, [isOpeningResolvedProject]);

  const requestOpenProject = useCallback(async (
    path: string,
    options?: OpenProjectOptions,
  ) => {
    const unavailableReason = getShellTabContributionUnavailableReason(
      useContributionRegistryStore.getState().snapshot,
      SHELL_TAB_CONTRIBUTIONS.project,
    );
    if (unavailableReason) {
      showToast({
        title: '项目管理器不可用',
        message: unavailableReason,
        tone: 'warning',
      });
      return false;
    }

    try {
      const report = await inspectProjectLocation(path);
      if (report.status === 'ready') {
        await openProjectSession(path, options);
        setPendingProjectOpen(null);
        setProjectLocationCandidates([]);
        setProjectLocationSearchError(null);
        setHasSearchedProjectLocation(false);
        return true;
      }

      setProjectLocationCandidates([]);
      setProjectLocationSearchError(null);
      setHasSearchedProjectLocation(false);
      setPendingProjectOpen({ path, options, report });
      return false;
    } catch (error) {
      console.error('Failed to inspect project location:', path, error);
      showToast({
        title: '项目位置检查失败',
        message: String(error),
        tone: 'error',
      });
      return false;
    }
  }, [openProjectSession, showToast]);

  const openResolvedProject = useCallback(async () => {
    const pending = pendingProjectOpen;
    if (!pending) {
      return;
    }

    setIsOpeningResolvedProject(true);
    try {
      await openProjectSession(pending.path, pending.options);
      setPendingProjectOpen(null);
      setProjectLocationCandidates([]);
      setProjectLocationSearchError(null);
      setHasSearchedProjectLocation(false);
    } catch (error) {
      console.error('Failed to initialize or repair project:', pending.path, error);
      showToast({
        title: pending.report.canInitialize ? '项目初始化失败' : '项目修复失败',
        message: String(error),
        tone: 'error',
      });
    } finally {
      setIsOpeningResolvedProject(false);
    }
  }, [openProjectSession, pendingProjectOpen, showToast]);

  const searchForProjectLocation = useCallback(async () => {
    const pending = pendingProjectOpen;
    if (!pending || pending.report.status !== 'missingDirectory') {
      return;
    }

    setIsSearchingProjectLocation(true);
    setProjectLocationSearchError(null);
    setHasSearchedProjectLocation(true);
    try {
      const searchRoots = projectsRootDir ? [projectsRootDir] : [];
      const candidates = await findProjectLocationCandidates(pending.path, searchRoots);
      setProjectLocationCandidates(candidates);
    } catch (error) {
      console.error('Failed to find project location:', pending.path, error);
      setProjectLocationSearchError(String(error));
    } finally {
      setIsSearchingProjectLocation(false);
    }
  }, [pendingProjectOpen, projectsRootDir]);

  const selectResolvedProjectLocation = useCallback((path: string) => {
    const options = pendingProjectOpen?.options;
    void requestOpenProject(path, options);
  }, [pendingProjectOpen?.options, requestOpenProject]);

  const handleOpenProject = useCallback(async (path: string) => {
    await requestOpenProject(path);
  }, [requestOpenProject]);

  const restorePersistedSession = useCallback(async (sessionSnapshot: PersistedAppSession) => {
    let restoredAnything = false;
    let unresolvedProject: PendingProjectOpen | null = null;
    const projectSessionMap = new Map(
      sessionSnapshot.projects.map((project) => [normalizeProjectPath(project.projectPath), project] as const),
    );

    const contributionRegistry = useContributionRegistryStore.getState().snapshot;
    const canRestoreLan = isShellTabContributionAvailable(
      contributionRegistry,
      SHELL_TAB_CONTRIBUTIONS.lan,
    );
    const canRestoreProjects = isShellTabContributionAvailable(
      contributionRegistry,
      SHELL_TAB_CONTRIBUTIONS.project,
    );

    if (canRestoreLan && sessionSnapshot.utilityTabs.includes('lan')) {
      useShellTabStore.getState().openLanTab();
      restoredAnything = true;
    }

    for (const projectTab of canRestoreProjects ? sessionSnapshot.projectTabs : []) {
      try {
        const report = await inspectProjectLocation(projectTab.projectPath);
        if (report.status !== 'ready') {
          unresolvedProject ??= {
            path: projectTab.projectPath,
            options: { skipRecentTracking: true },
            report,
          };
          continue;
        }

        const session = await openProjectSession(projectTab.projectPath, {
          skipRecentTracking: true,
        });

        const persistedProjectSession = projectSessionMap.get(
          normalizeProjectPath(projectTab.projectPath),
        );

        if (persistedProjectSession) {
          const projectState = session.projectStore.getState();

          if (persistedProjectSession.showExcludedFiles !== projectState.showExcludedFiles) {
            projectState.toggleShowExcludedFiles();
          }

          if (
            persistedProjectSession.currentPath &&
            persistedProjectSession.currentPath !== projectState.projectPath
          ) {
            await projectState.loadDirectory(persistedProjectSession.currentPath);
          }

          await restoreWorkspaceSession(session.workspaceTabStore, persistedProjectSession);
        }

        restoredAnything = true;
      } catch (error) {
        console.error('Failed to restore project session:', projectTab.projectPath, error);
      }
    }

    if (unresolvedProject) {
      setProjectLocationCandidates([]);
      setProjectLocationSearchError(null);
      setHasSearchedProjectLocation(false);
      setPendingProjectOpen(unresolvedProject);
    }

    for (const standaloneWindow of sessionSnapshot.standaloneWindows) {
      try {
        await restoreStandaloneWindow(standaloneWindow);
        restoredAnything = true;
      } catch (error) {
        console.error('Failed to restore standalone window:', standaloneWindow, error);
      }
    }

    if (sessionSnapshot.activeTab.type === 'project' && canRestoreProjects) {
      const activeProjectTab = useShellTabStore
        .getState()
        .findProjectTab(sessionSnapshot.activeTab.projectPath);

      if (activeProjectTab) {
        useShellTabStore.getState().activateTab(activeProjectTab.id);
      }
    } else if (sessionSnapshot.activeTab.type === 'lan' && canRestoreLan) {
      useShellTabStore.getState().openLanTab();
    } else {
      const homeTab = useShellTabStore.getState().tabs.find((tab) => tab.type === 'home');
      if (homeTab) {
        useShellTabStore.getState().activateTab(homeTab.id);
      }
    }

    return restoredAnything || unresolvedProject !== null;
  }, [openProjectSession]);

  useEffect(() => {
    const previousAvailability = projectShellAvailabilityRef.current;
    projectShellAvailabilityRef.current = projectShellAvailable;

    if (!isSettingsLoaded || !hasHandledStartupProjectRef.current) {
      return;
    }

    if (previousAvailability === projectShellAvailable) {
      return;
    }

    if (!projectShellAvailable) {
      if (sessionPersistTimerRef.current !== null) {
        window.clearTimeout(sessionPersistTimerRef.current);
        sessionPersistTimerRef.current = null;
      }

      suspendedProjectSessionRef.current = createPersistedAppSessionSnapshot();
      suppressSessionPersistenceRef.current = true;

      const shellState = useShellTabStore.getState();
      const projectTabs = shellState.tabs.filter(
        (tab) => tab.type === 'project' && tab.projectPath,
      );
      shellState.closeContributionTabs([SHELL_TAB_CONTRIBUTIONS.project.id]);
      projectTabs.forEach((tab) => {
        void releaseProjectSession(tab.projectPath!);
      });
      setPendingProjectOpen(null);
      setProjectLocationCandidates([]);
      setProjectLocationSearchError(null);
      setHasSearchedProjectLocation(false);
      return;
    }

    const suspendedSession = suspendedProjectSessionRef.current;
    if (!suspendedSession) {
      suppressSessionPersistenceRef.current = false;
      schedulePersistAppSession();
      return;
    }

    let cancelled = false;
    const restoreSuspendedProjects = async () => {
      isRestoringSessionRef.current = true;
      try {
        await restorePersistedSession(suspendedSession);
      } finally {
        isRestoringSessionRef.current = false;
        if (cancelled) {
          return;
        }
        suspendedProjectSessionRef.current = null;
        suppressSessionPersistenceRef.current = false;
        schedulePersistAppSession();
      }
    };

    void restoreSuspendedProjects();
    return () => {
      cancelled = true;
    };
  }, [
    createPersistedAppSessionSnapshot,
    isSettingsLoaded,
    projectShellAvailable,
    releaseProjectSession,
    restorePersistedSession,
    schedulePersistAppSession,
  ]);

  useEffect(() => {
    if (!isSettingsLoaded || hasHandledStartupProjectRef.current) {
      return;
    }

    hasHandledStartupProjectRef.current = true;

    const bootstrapStartupSession = async () => {
      isRestoringSessionRef.current = true;

      try {
        if (!autoOpenLastProject) {
          return;
        }

        const persistedSession = await loadPersistedAppSession();
        const currentProfile = useWorkspaceProfileStore.getState().snapshot?.currentProfile;
        if (
          persistedSession
          && currentProfile
          && getAppSessionProfileCompatibility(persistedSession, {
            id: currentProfile.id,
            revision: currentProfile.revision ?? 1,
          }) === 'mismatch'
        ) {
          showToast({
            title: '会话已按当前装配方案恢复',
            message: `上次会话保存于 ${persistedSession.profile?.id} r${persistedSession.profile?.revision}；当前为 ${currentProfile.id} r${currentProfile.revision}，不可用的功能页已忽略。`,
            tone: 'info',
          });
        }
        const restoredFromSession = persistedSession
          ? await restorePersistedSession(persistedSession)
          : false;

        if (restoredFromSession || recentProjects.length === 0) {
          return;
        }

        const projectManagerUnavailable = getShellTabContributionUnavailableReason(
          useContributionRegistryStore.getState().snapshot,
          SHELL_TAB_CONTRIBUTIONS.project,
        );
        if (projectManagerUnavailable) {
          return;
        }

        const [latestProject] = [...recentProjects].sort((left, right) => right.openedAt - left.openedAt);
        if (!latestProject?.path) {
          return;
        }

        await requestOpenProject(latestProject.path, {
          skipRecentTracking: true,
        });
      } finally {
        isRestoringSessionRef.current = false;
        isSessionPersistenceReadyRef.current = true;
        schedulePersistAppSession();
      }
    };

    void bootstrapStartupSession();
  }, [
    autoOpenLastProject,
    isSettingsLoaded,
    requestOpenProject,
    recentProjects,
    restorePersistedSession,
    schedulePersistAppSession,
    showToast,
  ]);

  useEffect(() => {
    let isActive = true;
    let unlisten: (() => void) | null = null;

    const registerReturnListener = async () => {
      try {
        unlisten = await listen<StandaloneReturnToWorkspacePayload>(
          STANDALONE_RETURN_TO_WORKSPACE_EVENT,
          async (event) => {
            const payload = event.payload;
            if (!payload?.projectPath || !payload?.filePath) {
              showToast({
                title: '回归失败',
                message: '缺少项目路径或文件路径。',
                tone: 'error',
              });
              return;
            }

            try {
              await handleOpenProject(payload.projectPath);
              const session = sessionsRef.current.get(normalizeProjectPath(payload.projectPath));
              if (!session) {
                throw new Error('未找到目标项目会话');
              }

              const openedTabId = payload.fileType === 'directory'
                ? session.workspaceTabStore.getState().openDirectoryInTab(payload.filePath)
                : await session.workspaceTabStore.getState().openFileInTab(
                    payload.filePath,
                    {
                      editorSnapshot:
                        payload.fileType === 'text' ? payload.textEditorSnapshot : undefined,
                    },
                  );
              if (!openedTabId) {
                throw new Error('该文件类型暂不支持回归到项目标签页');
              }

              showToast({
                title: '已回归项目',
                message: getFileNameFromPath(payload.filePath),
                tone: 'success',
              });
            } catch (error) {
              console.error('Failed to return detached window to workspace tab:', error);
              showToast({
                title: '回归失败',
                message: String(error),
                tone: 'error',
              });
            }
          },
        );

        if (!isActive && unlisten) {
          await unlisten();
          unlisten = null;
        }
      } catch (error) {
        console.error('Failed to register standalone return listener:', error);
      }
    };

    void registerReturnListener();

    return () => {
      isActive = false;
      if (unlisten) {
        void unlisten();
      }
    };
  }, [handleOpenProject, showToast]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;

    const registerPluginControlListener = async () => {
      try {
        unlisten = await listen<{ taskId: string; message: PluginControlMessage }>('task-control', async (event) => {
          const { taskId, message } = event.payload;
          const task = useTaskStore.getState().tasks.find((item) => item.id === taskId);

          if (!task) {
            return;
          }

          if (message.type === 'toast' && message.message) {
            showToast({
              title: message.title || '插件提示',
              message: message.message,
              tone: message.tone || 'info',
            });
          }

          if (message.type === 'refresh') {
            const session = sessionsRef.current.get(normalizeProjectPath(task.projectPath));
            if (session) {
              try {
                await session.projectStore.getState().refresh();
              } catch (error) {
                console.error('Failed to refresh project after plugin control event:', error);
              }
            }
          }

          if (
            message.type === 'confirm'
            && message.requestId
            && message.message
            && task.script.kind === 'plugin-action'
          ) {
            const payload = message.data && typeof message.data === 'object'
              ? message.data as Record<string, unknown>
              : null;
            const items = Array.isArray(payload?.items)
              ? payload.items.map((item) => String(item))
              : [];

            setPluginConfirmDialog({
              isOpen: true,
              task,
              requestId: message.requestId,
              title: message.title || '插件确认',
              message: message.message,
              confirmText: message.confirmText || '确认',
              cancelText: message.cancelText || '取消',
              items,
              data: message.data,
            });
          }
        });

        if (!active && unlisten) {
          await unlisten();
          unlisten = null;
        }
      } catch (error) {
        console.error('Failed to register plugin control listener:', error);
      }
    };

    void registerPluginControlListener();

    return () => {
      active = false;
      if (unlisten) {
        void unlisten();
      }
    };
  }, [showToast]);

  const closePluginConfirmDialog = () => {
    setPluginConfirmDialog((state) => ({
      ...state,
      isOpen: false,
      task: null,
      requestId: '',
      message: '',
      items: [],
      data: undefined,
    }));
  };

  const handleCancelPluginConfirm = () => {
    if (pluginConfirmDialog.task) {
      useTaskStore.getState().updateTaskOutput(
        pluginConfirmDialog.task.id,
        '[plugin-confirm-cancelled] 用户取消了插件确认操作',
      );
    }
    showToast({
      title: pluginConfirmDialog.title || '插件确认',
      message: '已取消本次插件操作。',
      tone: 'warning',
    });
    closePluginConfirmDialog();
  };

  const handleConfirmPluginAction = () => {
    const task = pluginConfirmDialog.task;
    if (!task || task.script.kind !== 'plugin-action') {
      closePluginConfirmDialog();
      return;
    }

    const interactionResponse: PluginInteractionResponse = {
      requestId: pluginConfirmDialog.requestId,
      approved: true,
      data: pluginConfirmDialog.data,
    };
    const existingResponses = task.script.interactionResponses ?? [];
    const nextResponses = [
      ...existingResponses.filter((response) => response.requestId !== interactionResponse.requestId),
      interactionResponse,
    ];

    addTask({
      projectPath: task.projectPath,
      name: task.name,
      subName: task.subName,
      script: {
        ...task.script,
        interactionResponses: nextResponses,
      },
      priority: task.priority,
      maxRetries: task.maxRetries,
      timeout: task.timeout,
      dependencies: task.dependencies,
    });

    showToast({
      title: pluginConfirmDialog.title || '插件确认',
      message: '已确认，插件任务开始执行。',
      tone: 'success',
    });
    closePluginConfirmDialog();
  };

  useEffect(() => {
    const unsubscribeStandaloneWindows = subscribeTrackedStandaloneWindows(() => {
      schedulePersistAppSession();
    });

    return () => {
      unsubscribeStandaloneWindows();
    };
  }, [schedulePersistAppSession]);

  useEffect(() => {
    schedulePersistAppSession();
  }, [
    activeTabId,
    activeWorkspaceProfileId,
    activeWorkspaceProfileRevision,
    schedulePersistAppSession,
    tabs,
  ]);

  useEffect(() => {
    return () => {
      if (sessionPersistTimerRef.current !== null) {
        window.clearTimeout(sessionPersistTimerRef.current);
        sessionPersistTimerRef.current = null;
      }

      sessionSubscriptionsRef.current.forEach((subscriptions) => {
        subscriptions.unsubscribeProject();
        subscriptions.unsubscribeWorkspace();
      });
      sessionSubscriptionsRef.current.clear();
    };
  }, []);

  const handleCloseShellTab = async (tabId: string) => {
    const closingTab = tabs.find((tab) => tab.id === tabId);
    closeTab(tabId);

    if (closingTab?.type === 'project' && closingTab.projectPath) {
      await releaseProjectSession(closingTab.projectPath);
    }
  };

  const openBuiltinTool = useCallback((toolId: BuiltinToolId) => {
    const tool = BUILTIN_TOOL_BY_ID.get(toolId);
    if (!tool) {
      return;
    }
    const contributionRegistry = useContributionRegistryStore.getState().snapshot;
    const unavailableReason = getContributionUnavailableReason(
      contributionRegistry,
      tool.contribution,
    );
    if (unavailableReason) {
      showToast({
        title: `${tool.title}不可用`,
        message: unavailableReason,
        tone: 'warning',
      });
      return;
    }

    const shellState = useShellTabStore.getState();
    const currentShellTab = shellState.tabs.find((tab) => tab.id === shellState.activeTabId);
    const currentProjectSession = currentShellTab?.type === 'project' && currentShellTab.projectPath
      ? sessionsRef.current.get(normalizeProjectPath(currentShellTab.projectPath)) ?? null
      : null;

    const requireProjectSession = () => {
      if (currentProjectSession) {
        return currentProjectSession;
      }

      showToast({
        title: '需要打开项目',
        message: '这个功能需要当前活动项目，请先打开或切换到项目标签页。',
        tone: 'warning',
      });
      return null;
    };

    const dialogOpeners: Record<BuiltinToolDialogId, () => void> = {
      'python-environments': () => setIsPythonEnvOpen(true),
      'task-center': () => setIsTaskCenterOpen(true),
      settings: () => setIsSettingsOpen(true),
      'blender-file-parser': () => {
        const selectedBlendFiles = currentProjectSession
          ? Array.from(currentProjectSession.projectStore.getState().selectedFiles)
            .filter((path) => path.toLocaleLowerCase().endsWith('.blend'))
          : [];
        setBlenderParserInitialFilePath(selectedBlendFiles.length === 1 ? selectedBlendFiles[0] : null);
        setIsBlenderFileParserOpen(true);
      },
      'script-developer-studio': () => setIsScriptDeveloperWorkbenchOpen(true),
    };

    const target = tool.openTarget;
    switch (target.type) {
      case 'workspaceTab': {
        const workspaceContribution = WORKSPACE_TAB_CONTRIBUTION_BY_ID.get(target.contributionId);
        const reason = workspaceContribution
          ? getWorkspaceTabContributionUnavailableReason(contributionRegistry, workspaceContribution)
          : '工作区贡献未注册';
        if (reason) {
          showToast({ title: `${tool.title}不可用`, message: reason, tone: 'warning' });
          return;
        }
        const session = requireProjectSession();
        session?.workspaceTabStore
          .getState()
          .openWorkspaceContributionTab(target.contributionId);
        break;
      }
      case 'shellTab': {
        const shellContribution = SHELL_TAB_CONTRIBUTION_BY_ID.get(target.contributionId);
        const reason = shellContribution
          ? getShellTabContributionUnavailableReason(contributionRegistry, shellContribution)
          : '主标签贡献未注册';
        if (reason) {
          showToast({ title: `${tool.title}不可用`, message: reason, tone: 'warning' });
          return;
        }
        shellState.openShellContributionTab(target.contributionId);
        break;
      }
      case 'dialog':
        dialogOpeners[target.dialogId]();
        break;
      case 'event': {
        if (tool.requiresProject && !requireProjectSession()) {
          return;
        }
        window.dispatchEvent(new Event(target.eventName));
        break;
      }
      case 'command':
        void invoke(target.command).catch((error) => {
          showToast({
            title: target.errorTitle,
            message: String(error),
            tone: 'error',
          });
        });
        break;
    }
  }, [showToast]);

  const toastStyles = {
    info: 'border-blue-200 bg-white text-gray-900',
    success: 'border-green-200 bg-white text-gray-900',
    warning: 'border-yellow-200 bg-white text-gray-900',
    error: 'border-red-200 bg-white text-gray-900',
  };
  const toastAccentStyles = {
    info: 'bg-blue-500',
    success: 'bg-green-500',
    warning: 'bg-yellow-500',
    error: 'bg-red-500',
  };

  return (
    <div className="h-screen flex flex-col bg-gray-50 dark:bg-gray-900">
      <div className="flex min-h-12 items-center border-b border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
        <div className="min-w-0 flex-1 overflow-hidden">
          {activeProjectSession ? (
            <ProjectSessionProvider
              projectStore={activeProjectSession.projectStore}
              workspaceTabStore={activeProjectSession.workspaceTabStore}
            >
              <Toolbar />
            </ProjectSessionProvider>
          ) : (
            <div className="h-12 px-3" />
          )}
        </div>

        <div className="flex h-12 shrink-0 items-center gap-1.5 border-l border-gray-200 px-2 dark:border-gray-700">
          <PinnedToolsToolbar
            onOpenTool={openBuiltinTool}
            onOpenScriptSurface={openScriptSurface}
          />
          <DevelopmentReloadControl
            onOpenDeveloperWorkbench={() => setIsScriptDeveloperWorkbenchOpen(true)}
          />
          <button
            type="button"
            onClick={() => setIsRecoverySettingsOpen(true)}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-gray-500 transition-colors hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            title="维护中心"
          >
            <ShieldCheck className="h-4 w-4" />
          </button>
          <LauncherButton
            hasActiveProject={Boolean(activeProjectSession)}
            activeProjectName={activeProjectSession?.projectStore.getState().projectName || activeShellTab?.title}
            onOpenTool={openBuiltinTool}
            onOpenScriptSurface={openScriptSurface}
          />
        </div>
      </div>

      <ShellTabBar
        tabs={tabs}
        activeTabId={activeTabId}
        onActivateTab={activateTab}
        onCloseTab={handleCloseShellTab}
        onReorderTabs={reorderTabs}
      />

      {profileNavigationKind !== 'side-bar' ? (
        <ProfileNavigationBar
          items={profileNavigationItems}
          kind={profileNavigationKind}
          activeContributionId={activeShellTab?.contributionId}
          homeSurfaceId={activeWorkspaceProfile?.shellLayout?.home}
          homeActive={activeShellTab?.type === 'home'}
          onOpen={openProfileNavigation}
          onOpenHome={openProfileHome}
        />
      ) : (
        <div className="md:hidden">
          <ProfileNavigationBar
            items={profileNavigationItems}
            kind="top-bar"
            activeContributionId={activeShellTab?.contributionId}
            homeSurfaceId={activeWorkspaceProfile?.shellLayout?.home}
            homeActive={activeShellTab?.type === 'home'}
            onOpen={openProfileNavigation}
            onOpenHome={openProfileHome}
          />
        </div>
      )}

      <div className="flex min-h-0 flex-1 overflow-hidden">
        {profileNavigationKind === 'side-bar' ? (
          <ProfileNavigationBar
            items={profileNavigationItems}
            kind="side-bar"
            activeContributionId={activeShellTab?.contributionId}
            homeSurfaceId={activeWorkspaceProfile?.shellLayout?.home}
            homeActive={activeShellTab?.type === 'home'}
            onOpen={openProfileNavigation}
            onOpenHome={openProfileHome}
          />
        ) : null}
        <div className="min-w-0 flex-1 overflow-hidden">
          {contributionShellTabs.map((tab) => {
            const isActive = tab.id === activeTabId;
            return (
              <div key={tab.id} className={isActive ? 'h-full' : 'hidden'}>
                <ContributedShellSurface tab={tab} isActive={isActive} />
              </div>
            );
          })}
          {!isContributionShellActive && activeProjectSession ? (
            <ProjectSessionProvider
              projectStore={activeProjectSession.projectStore}
              workspaceTabStore={activeProjectSession.workspaceTabStore}
            >
              <ProjectWorkspace />
            </ProjectSessionProvider>
          ) : !isContributionShellActive ? (
            <ProfileHomeSurface
              onOpenProject={handleOpenProject}
              settingsLoaded={isSettingsLoaded}
              onOpenRecovery={() => setIsRecoverySettingsOpen(true)}
            />
          ) : null}
        </div>
      </div>

      <PythonEnvManager
        isOpen={isPythonEnvOpen && pythonToolAvailable}
        onClose={() => setIsPythonEnvOpen(false)}
      />

      {activeProjectSession ? (
        <ProjectSessionProvider
          projectStore={activeProjectSession.projectStore}
          workspaceTabStore={activeProjectSession.workspaceTabStore}
        >
          <TaskPanel isOpen={isTaskCenterOpen && taskToolAvailable} onClose={() => setIsTaskCenterOpen(false)} />
          <SettingsPanel
            isOpen={isSettingsOpen && settingsToolAvailable}
            onClose={() => setIsSettingsOpen(false)}
            defaultScope="project"
            onOpenProject={handleOpenProject}
          />
        </ProjectSessionProvider>
      ) : (
        <>
          <TaskPanel isOpen={isTaskCenterOpen && taskToolAvailable} onClose={() => setIsTaskCenterOpen(false)} />
          <SettingsPanel
            isOpen={isSettingsOpen && settingsToolAvailable}
            onClose={() => setIsSettingsOpen(false)}
            defaultScope="global"
            onOpenProject={handleOpenProject}
          />
        </>
      )}

      <RecoverySettingsPanel
        isOpen={isRecoverySettingsOpen}
        onClose={() => setIsRecoverySettingsOpen(false)}
      />

      <BlenderFileParserDialog
        isOpen={isBlenderFileParserOpen && blenderParserToolAvailable}
        onClose={() => setIsBlenderFileParserOpen(false)}
        projectPath={activeProjectSession?.projectStore.getState().projectPath}
        projectName={activeProjectSession?.projectStore.getState().projectName}
        initialFilePath={blenderParserInitialFilePath}
        onOpenInWorkspace={activeProjectSession
          ? (filePath) => activeProjectSession.workspaceTabStore.getState().openFileInTab(filePath)
          : undefined}
      />

      <ScriptDeveloperWorkbench
        isOpen={isScriptDeveloperWorkbenchOpen && scriptAutomationToolAvailable}
        onClose={() => setIsScriptDeveloperWorkbenchOpen(false)}
        projectPath={activeProjectSession?.projectStore.getState().projectPath}
      />

      <Dialog
        isOpen={Boolean(activeScriptSurface) && scriptAutomationToolAvailable}
        onClose={() => setActiveScriptSurface(null)}
        title={activeScriptSurface?.title ?? '组件页面'}
        size="2xl"
        contentClassName="h-[680px] min-h-0 overflow-hidden p-0"
      >
        {activeScriptSurface ? (
          <ScriptSurfaceFrame
            componentId={activeScriptSurface.componentId}
            surfaceId={activeScriptSurface.surfaceId}
            projectPath={activeProjectSession?.projectStore.getState().projectPath}
          />
        ) : null}
      </Dialog>

      <ProjectLocationDialog
        report={pendingProjectOpen?.report ?? null}
        candidates={projectLocationCandidates}
        isSearching={isSearchingProjectLocation}
        hasSearched={hasSearchedProjectLocation}
        searchError={projectLocationSearchError}
        isOpening={isOpeningResolvedProject}
        onClose={closeProjectLocationDialog}
        onInitialize={() => void openResolvedProject()}
        onRepair={() => void openResolvedProject()}
        onSearch={() => void searchForProjectLocation()}
        onSelectLocation={selectResolvedProjectLocation}
      />

      {toast.isOpen && (
        <div className="fixed right-4 top-16 z-[120] w-[360px] max-w-[calc(100vw-2rem)]">
          <div className={`relative overflow-hidden rounded-xl border shadow-xl ${toastStyles[toast.tone]}`}>
            <div className={`absolute left-0 top-0 h-full w-1 ${toastAccentStyles[toast.tone]}`} />
            <div className="flex items-start gap-3 px-4 py-3 pl-5">
              <div className="min-w-0 flex-1">
                <p className="text-sm font-semibold">{toast.title}</p>
                <p className="mt-1 text-sm text-gray-600">{toast.message}</p>
              </div>
              <button
                onClick={hideToast}
                className="rounded-md p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600 transition-colors"
                title="关闭提示"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
          </div>
        </div>
      )}

      <Dialog
        isOpen={pluginConfirmDialog.isOpen}
        onClose={handleCancelPluginConfirm}
        title={pluginConfirmDialog.title}
        size="lg"
        footer={
          <>
            <button
              onClick={handleCancelPluginConfirm}
              className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors"
            >
              {pluginConfirmDialog.cancelText}
            </button>
            <button
              onClick={handleConfirmPluginAction}
              className="px-4 py-2 text-sm bg-red-600 hover:bg-red-700 text-white rounded-lg transition-colors"
            >
              {pluginConfirmDialog.confirmText}
            </button>
          </>
        }
      >
        <div className="space-y-4">
          <p className="text-sm text-gray-700 dark:text-gray-300 whitespace-pre-line">
            {pluginConfirmDialog.message}
          </p>
          {pluginConfirmDialog.items.length > 0 ? (
            <div className="rounded-xl border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/60">
              <div className="border-b border-gray-200 dark:border-gray-700 px-4 py-3 text-sm font-medium text-gray-900 dark:text-gray-100">
                待处理文件 ({pluginConfirmDialog.items.length})
              </div>
              <div className="max-h-72 overflow-auto px-4 py-3">
                <div className="space-y-2">
                  {pluginConfirmDialog.items.map((item) => (
                    <div
                      key={item}
                      className="rounded-lg bg-white dark:bg-gray-900 px-3 py-2 text-sm text-gray-700 dark:text-gray-300 break-all"
                    >
                      {item}
                    </div>
                  ))}
                </div>
              </div>
            </div>
          ) : null}
        </div>
      </Dialog>
    </div>
  );
}
