import { useEffect, useMemo, useRef, useState } from 'react';
import { History, MessageCircle, Terminal, X } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { WelcomeScreen } from '../WelcomeScreen';
import { P2PChat } from '../P2PChat';
import { PythonEnvManager } from '../PythonEnvManager';
import { TaskButton } from '../TaskButton';
import { LauncherButton } from '../Launcher';
import { Toolbar } from './Toolbar';
import { ProjectWorkspace } from './ProjectWorkspace';
import { ProjectSessionProvider } from './ProjectSessionProvider';
import { ShellTabBar } from '../shell/ShellTabBar';
import { createProjectStore, type ProjectStoreApi } from '../../stores/projectStore';
import { createWorkspaceTabStore, type WorkspaceTabStoreApi, useWorkspaceTabStore } from '../../stores/workspaceTabStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useUiStore } from '../../stores/uiStore';
import { useShellTabStore, normalizeProjectPath } from '../../stores/shellTabStore';

interface ProjectSession {
  projectStore: ProjectStoreApi;
  workspaceTabStore: WorkspaceTabStoreApi;
}

function getProjectNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || 'Project';
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
  const toast = useUiStore((state) => state.toast);
  const hideToast = useUiStore((state) => state.hideToast);
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
  const sessionsRef = useRef<Map<string, ProjectSession>>(new Map());
  const hasHandledStartupProjectRef = useRef(false);

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

  useEffect(() => {
    if (!isSettingsLoaded || hasHandledStartupProjectRef.current) {
      return;
    }

    hasHandledStartupProjectRef.current = true;

    if (!autoOpenLastProject || recentProjects.length === 0) {
      return;
    }

    const [latestProject] = [...recentProjects].sort((left, right) => right.openedAt - left.openedAt);
    if (!latestProject?.path) {
      return;
    }

    void handleOpenProject(latestProject.path);
  }, [autoOpenLastProject, isSettingsLoaded, recentProjects]);

  const activeProjectSession = activeShellTab?.type === 'project' && activeShellTab.projectPath
    ? sessionsRef.current.get(normalizeProjectPath(activeShellTab.projectPath)) ?? null
    : null;

  const handleOpenProject = async (path: string) => {
    const normalizedPath = normalizeProjectPath(path);
    let session = sessionsRef.current.get(normalizedPath);

    if (!session) {
      session = {
        projectStore: createProjectStore(),
        workspaceTabStore: createWorkspaceTabStore(),
      };
      await session.projectStore.getState().setProject(path);
      sessionsRef.current.set(normalizedPath, session);
    }

    const projectName = session.projectStore.getState().projectName || getProjectNameFromPath(path);
    openProjectTab(path, projectName);
    await addRecentProject(path, projectName);
  };

  const handleCloseShellTab = (tabId: string) => {
    const closingTab = tabs.find((tab) => tab.id === tabId);
    closeTab(tabId);

    if (closingTab?.type === 'project' && closingTab.projectPath) {
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
              <Toolbar />
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
          <WelcomeScreen onOpenProject={handleOpenProject} />
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
    </div>
  );
}
