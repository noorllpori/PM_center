import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { History, MessageCircle, Terminal, X } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { WelcomeScreen } from '../WelcomeScreen';
import { P2PChat } from '../P2PChat';
import { PythonEnvManager } from '../PythonEnvManager';
import { openStandaloneImageViewer } from '../image-viewer/openStandaloneImageViewer';
import { TaskButton } from '../TaskButton';
import { LauncherButton } from '../Launcher';
import { openStandaloneTextEditor } from '../text-editor/openStandaloneTextEditor';
import { openStandaloneVideoPlayer } from '../video-player/openStandaloneVideoPlayer';
import { Toolbar, TOOLBAR_SEARCH_FOCUS_EVENT } from './Toolbar';
import { ProjectWorkspace } from './ProjectWorkspace';
import { ProjectSessionProvider } from './ProjectSessionProvider';
import { ShellTabBar } from '../shell/ShellTabBar';
import { Dialog } from '../Dialog';
import { createProjectStore, type ProjectStoreApi } from '../../stores/projectStore';
import { useTaskStore } from '../../stores/taskStore';
import { createWorkspaceTabStore, type WorkspaceTabStoreApi, useWorkspaceTabStore } from '../../stores/workspaceTabStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useUiStore } from '../../stores/uiStore';
import { useShellTabStore, normalizeProjectPath } from '../../stores/shellTabStore';
import {
  createDefaultPersistedAppSession,
  dedupeStandaloneWindows,
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

const SESSION_PERSIST_DEBOUNCE_MS = 180;

function getProjectNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || 'Project';
}

function getFileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function getPersistedWorkspaceTabKey(tab: PersistedWorkspaceTab | PersistedWorkspaceActiveTab) {
  return tab.type === 'logs' || tab.type === 'files'
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

    if (tab.type === 'logs') {
      return [{ type: 'logs', title: tab.title }];
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
    activeTab?.type === 'logs'
      ? { type: 'logs' }
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

  for (const tab of session.tabs) {
    if (tab.type === 'logs') {
      const tabId = workspaceTabStore.getState().openLogsTab();
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

function ProjectLogsButton({ onOpen }: { onOpen: () => void }) {
  const activeWorkspaceType = useWorkspaceTabStore((state) => {
    const activeTab = state.tabs.find((tab) => tab.id === state.activeTabId);
    return activeTab?.type;
  });

  return (
    <button
      onClick={onOpen}
      className={`p-2 rounded-lg transition-colors ${
        activeWorkspaceType === 'logs'
          ? 'bg-amber-50 text-amber-600 dark:bg-amber-900/20 dark:text-amber-300'
          : 'text-gray-500 hover:text-amber-600 dark:text-gray-400 dark:hover:text-amber-300 hover:bg-amber-50 dark:hover:bg-amber-900/20'
      }`}
      title="日志"
    >
      <History className="w-5 h-5" />
    </button>
  );
}

export function FileManager() {
  const loadSettings = useSettingsStore((state) => state.loadSettings);
  const recentProjects = useSettingsStore((state) => state.recentProjects);
  const autoOpenLastProject = useSettingsStore((state) => state.autoOpenLastProject);
  const addRecentProject = useSettingsStore((state) => state.addRecentProject);
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

  const [isP2PChatOpen, setIsP2PChatOpen] = useState(false);
  const [isPythonEnvOpen, setIsPythonEnvOpen] = useState(false);
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
  const sessionsRef = useRef<Map<string, ProjectSession>>(new Map());
  const sessionSubscriptionsRef = useRef<Map<string, ProjectSessionSubscriptions>>(new Map());
  const sessionPersistTimerRef = useRef<number | null>(null);
  const hasHandledStartupProjectRef = useRef(false);
  const isRestoringSessionRef = useRef(false);
  const isSessionPersistenceReadyRef = useRef(false);

  useEffect(() => {
    let isActive = true;

    const initializeSettings = async () => {
      await loadSettings();
      if (isActive) {
        setIsSettingsLoaded(true);
      }
    };

    void initializeSettings();

    return () => {
      isActive = false;
    };
  }, [loadSettings]);

  useEffect(() => {
    if (!toast.isOpen) {
      return;
    }

    const timeout = window.setTimeout(() => {
      hideToast();
    }, toast.tone === 'error' ? 6000 : 3500);

    return () => window.clearTimeout(timeout);
  }, [hideToast, toast.isOpen, toast.tone]);

  const activeProjectSession = activeShellTab?.type === 'project' && activeShellTab.projectPath
    ? sessionsRef.current.get(normalizeProjectPath(activeShellTab.projectPath)) ?? null
    : null;

  const persistAppSession = useCallback(async () => {
    if (!isSessionPersistenceReadyRef.current || isRestoringSessionRef.current) {
      return;
    }

    const shellState = useShellTabStore.getState();
    const projectTabs = shellState.tabs
      .filter((tab) => tab.type === 'project' && tab.projectPath)
      .map((tab) => ({
        projectPath: tab.projectPath!,
        title: tab.title,
      }));

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
    const persistedSession: PersistedAppSession = {
      ...createDefaultPersistedAppSession(),
      projectTabs,
      activeTab:
        activeTab?.type === 'project' && activeTab.projectPath
          ? { type: 'project', projectPath: activeTab.projectPath }
          : { type: 'home' },
      projects,
      standaloneWindows: dedupeStandaloneWindows(getTrackedStandaloneWindows()),
    };

    await savePersistedAppSession(persistedSession);
  }, []);

  const schedulePersistAppSession = useCallback(() => {
    if (!isSessionPersistenceReadyRef.current || isRestoringSessionRef.current) {
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
    options?: {
      skipRecentTracking?: boolean;
    },
  ) => {
    const session = await ensureProjectSession(path);
    const projectName = session.projectStore.getState().projectName || getProjectNameFromPath(path);
    openProjectTab(path, projectName);
    if (!options?.skipRecentTracking) {
      await addRecentProject(path, projectName);
    }
    return session;
  }, [addRecentProject, ensureProjectSession, openProjectTab]);

  const handleOpenProject = useCallback(async (path: string) => {
    await openProjectSession(path);
  }, [openProjectSession]);

  const restorePersistedSession = useCallback(async (sessionSnapshot: PersistedAppSession) => {
    let restoredAnything = false;
    const projectSessionMap = new Map(
      sessionSnapshot.projects.map((project) => [normalizeProjectPath(project.projectPath), project] as const),
    );

    for (const projectTab of sessionSnapshot.projectTabs) {
      try {
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

    for (const standaloneWindow of sessionSnapshot.standaloneWindows) {
      try {
        await restoreStandaloneWindow(standaloneWindow);
        restoredAnything = true;
      } catch (error) {
        console.error('Failed to restore standalone window:', standaloneWindow, error);
      }
    }

    if (sessionSnapshot.activeTab.type === 'project') {
      const activeProjectTab = useShellTabStore
        .getState()
        .findProjectTab(sessionSnapshot.activeTab.projectPath);

      if (activeProjectTab) {
        useShellTabStore.getState().activateTab(activeProjectTab.id);
      }
    } else {
      const homeTab = useShellTabStore.getState().tabs.find((tab) => tab.type === 'home');
      if (homeTab) {
        useShellTabStore.getState().activateTab(homeTab.id);
      }
    }

    return restoredAnything;
  }, [openProjectSession]);

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
        const restoredFromSession = persistedSession
          ? await restorePersistedSession(persistedSession)
          : false;

        if (restoredFromSession || recentProjects.length === 0) {
          return;
        }

        const [latestProject] = [...recentProjects].sort((left, right) => right.openedAt - left.openedAt);
        if (!latestProject?.path) {
          return;
        }

        await openProjectSession(latestProject.path, {
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
    openProjectSession,
    recentProjects,
    restorePersistedSession,
    schedulePersistAppSession,
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

              const openedTabId = await session.workspaceTabStore.getState().openFileInTab(
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
  }, [activeTabId, schedulePersistAppSession, tabs]);

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

  const handleCloseShellTab = (tabId: string) => {
    const closingTab = tabs.find((tab) => tab.id === tabId);
    closeTab(tabId);

    if (closingTab?.type === 'project' && closingTab.projectPath) {
      unregisterSessionPersistence(closingTab.projectPath);
      sessionsRef.current.delete(normalizeProjectPath(closingTab.projectPath));
    }
  };

  const handleOpenLogsTab = () => {
    if (!activeProjectSession) {
      return;
    }

    activeProjectSession.workspaceTabStore.getState().openLogsTab();
  };

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
      <div className="flex items-center border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900">
        <div className="flex-1 min-w-0">
          {activeProjectSession ? (
            <ProjectSessionProvider
              projectStore={activeProjectSession.projectStore}
              workspaceTabStore={activeProjectSession.workspaceTabStore}
            >
              <Toolbar onOpenProject={handleOpenProject} />
            </ProjectSessionProvider>
          ) : (
            <div className="h-full px-3 py-2" />
          )}
        </div>

        <div className="flex items-center gap-2 px-3 border-l border-gray-200 dark:border-gray-700">
          {activeProjectSession && (
            <ProjectSessionProvider
              projectStore={activeProjectSession.projectStore}
              workspaceTabStore={activeProjectSession.workspaceTabStore}
            >
              <ProjectLogsButton onOpen={handleOpenLogsTab} />
            </ProjectSessionProvider>
          )}
          <button
            onClick={() => setIsPythonEnvOpen(true)}
            className="p-2 text-gray-500 hover:text-green-600 dark:text-gray-400 
                       dark:hover:text-green-400 hover:bg-green-50 dark:hover:bg-green-900/20 
                       rounded-lg transition-colors"
            title="Python 环境管理"
          >
            <Terminal className="w-5 h-5" />
          </button>
          <button
            onClick={() => setIsP2PChatOpen(true)}
            className="p-2 text-gray-500 hover:text-blue-600 dark:text-gray-400 
                       dark:hover:text-blue-400 hover:bg-blue-50 dark:hover:bg-blue-900/20 
                       rounded-lg transition-colors"
            title="局域网消息"
          >
            <MessageCircle className="w-5 h-5" />
          </button>
          {activeProjectSession ? (
            <ProjectSessionProvider
              projectStore={activeProjectSession.projectStore}
              workspaceTabStore={activeProjectSession.workspaceTabStore}
            >
              <TaskButton />
            </ProjectSessionProvider>
          ) : (
            <TaskButton />
          )}
          <LauncherButton />
        </div>
      </div>

      <ShellTabBar
        tabs={tabs}
        activeTabId={activeTabId}
        onActivateTab={activateTab}
        onCloseTab={handleCloseShellTab}
        onReorderTabs={reorderTabs}
      />

      <div className="flex-1 min-h-0 overflow-hidden">
        {activeProjectSession ? (
          <ProjectSessionProvider
            projectStore={activeProjectSession.projectStore}
            workspaceTabStore={activeProjectSession.workspaceTabStore}
          >
            <ProjectWorkspace />
          </ProjectSessionProvider>
        ) : (
          <WelcomeScreen
            onOpenProject={handleOpenProject}
            settingsLoaded={isSettingsLoaded}
          />
        )}
      </div>

      <P2PChat
        isOpen={isP2PChatOpen}
        onClose={() => setIsP2PChatOpen(false)}
      />

      <PythonEnvManager
        isOpen={isPythonEnvOpen}
        onClose={() => setIsPythonEnvOpen(false)}
      />

      {toast.isOpen && (
        <div className="fixed right-4 bottom-20 z-[120] w-[360px] max-w-[calc(100vw-2rem)]">
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
