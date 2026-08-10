import { useCallback, useEffect, useMemo, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { Eye, EyeOff, Folder, Settings } from 'lucide-react';
import { createProject, scanProjectsRoot, type ScannedProject } from '../../api/projects';
import {
  executeContributionCommand,
  type ContributionCommandHandlers,
} from '../../features/contributionCommands';
import {
  COMMAND_CONTRIBUTIONS,
  DATA_SOURCE_CONTRIBUTIONS,
  TOOL_CONTRIBUTIONS,
  WIDGET_CONTRIBUTIONS,
  isContributionAvailable,
} from '../../features/contributionRegistry';
import { useContributionRegistryStore } from '../../stores/contributionRegistryStore';
import { useSettingsStore } from '../../stores/settingsStore';
import type { JsonValue, ProfileSurface, WorkspaceProfileV1 } from '../../types/platform';
import {
  APP_AUTHOR_CONTACT,
  APP_AUTHOR_NAME,
  APP_NAME,
  APP_VERSION_TEXT,
} from '../../config/appMeta';
import nexoraLogo from '../../assets/nexora-logo.png';
import { AlertDialog, ConfirmDialog, Dialog } from '../Dialog';
import { SettingsPanel } from '../SettingsPanel';
import {
  ContributedWidget,
  type ContributedWidgetRuntime,
} from '../workspace/ContributedWidget';
import { UiExtensionSlot } from '../automation/UiExtensionSlot';

interface ProjectHomeCompositionProps {
  onOpenProject: (path: string) => Promise<void> | void;
  settingsLoaded: boolean;
  profile?: WorkspaceProfileV1;
  profileSurface?: ProfileSurface;
}

interface HomeWidgetPlacement {
  id: string;
  widgetId: string;
  dataSourceId?: string;
  region: 'sidebar' | 'content';
  order: number;
}

const DEFAULT_HOME_WIDGETS: HomeWidgetPlacement[] = [
  {
    id: 'project-directory',
    widgetId: WIDGET_CONTRIBUTIONS.projectDirectory.id,
    dataSourceId: DATA_SOURCE_CONTRIBUTIONS.projectDirectory.id,
    region: 'sidebar',
    order: 0,
  },
  {
    id: 'quick-actions',
    widgetId: WIDGET_CONTRIBUTIONS.projectQuickActions.id,
    dataSourceId: DATA_SOURCE_CONTRIBUTIONS.projectQuickActions.id,
    region: 'sidebar',
    order: 1,
  },
  {
    id: 'recent-projects',
    widgetId: WIDGET_CONTRIBUTIONS.recentProjects.id,
    dataSourceId: DATA_SOURCE_CONTRIBUTIONS.recentProjects.id,
    region: 'content',
    order: 0,
  },
  {
    id: 'project-catalog',
    widgetId: WIDGET_CONTRIBUTIONS.projectCatalog.id,
    dataSourceId: DATA_SOURCE_CONTRIBUTIONS.projectCatalog.id,
    region: 'content',
    order: 1,
  },
];

function getProjectDatePrefix(date = new Date()): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}${month}${day}@`;
}

function payloadText(payload: JsonValue | undefined, key: string) {
  if (!payload || Array.isArray(payload) || typeof payload !== 'object') {
    return null;
  }
  const value = payload[key];
  return typeof value === 'string' ? value : null;
}

function resolveHomeWidgets(
  profile: WorkspaceProfileV1 | undefined,
  surface: ProfileSurface | undefined,
) {
  const widgetsConfigured = surface?.settings?.widgetsConfigured === true;
  if (!surface || (!widgetsConfigured && !surface.widgets?.length)) {
    return DEFAULT_HOME_WIDGETS;
  }
  const dataSources = new Map((profile?.dataSources ?? []).map((source) => [source.id, source.source]));
  return (surface.widgets ?? [])
    .map<HomeWidgetPlacement>((widget) => ({
      id: widget.id,
      widgetId: widget.widget,
      dataSourceId: widget.dataSource ? dataSources.get(widget.dataSource) : undefined,
      region: widget.region === 'sidebar' ? 'sidebar' : 'content',
      order: widget.order ?? 0,
    }))
    .sort((left, right) => left.order - right.order || left.id.localeCompare(right.id));
}

export function ProjectHomeComposition({
  onOpenProject,
  settingsLoaded,
  profile,
  profileSurface,
}: ProjectHomeCompositionProps) {
  const {
    recentProjects,
    projectsRootDir,
    ignoredProjects,
    removeRecentProject,
    setProjectsRootDir,
    ignoreProject,
    unignoreProject,
  } = useSettingsStore();
  const contributionRegistry = useContributionRegistryStore((state) => state.snapshot);
  const settingsToolAvailable = isContributionAvailable(
    contributionRegistry,
    TOOL_CONTRIBUTIONS.settings,
  );
  const [scannedProjects, setScannedProjects] = useState<ScannedProject[]>([]);
  const [isScanning, setIsScanning] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [showIgnoredList, setShowIgnoredList] = useState(false);
  const [newProjectName, setNewProjectName] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [activeContentWidgetId, setActiveContentWidgetId] = useState('');
  const [confirmDialog, setConfirmDialog] = useState<{
    isOpen: boolean;
    title: string;
    message: string;
    onConfirm: () => void | Promise<void>;
  }>({ isOpen: false, title: '', message: '', onConfirm: () => undefined });
  const [alertDialog, setAlertDialog] = useState({
    isOpen: false,
    title: '提示',
    message: '',
  });

  const placements = useMemo(
    () => resolveHomeWidgets(profile, profileSurface),
    [profile, profileSurface],
  );
  const sidebarWidgets = placements.filter((widget) => widget.region === 'sidebar');
  const contentWidgets = placements.filter((widget) => (
    widget.region === 'content'
      && (
        widget.widgetId !== WIDGET_CONTRIBUTIONS.projectCatalog.id
        || (settingsLoaded && Boolean(projectsRootDir))
      )
  ));

  useEffect(() => {
    if (!settingsToolAvailable) {
      setShowSettings(false);
    }
  }, [settingsToolAvailable]);

  useEffect(() => {
    if (!contentWidgets.some((widget) => widget.id === activeContentWidgetId)) {
      setActiveContentWidgetId(contentWidgets[0]?.id ?? '');
    }
  }, [activeContentWidgetId, contentWidgets]);

  const scanProjects = useCallback(async () => {
    if (!projectsRootDir) {
      setScannedProjects([]);
      return;
    }
    setIsScanning(true);
    try {
      const projects = await scanProjectsRoot(projectsRootDir);
      setScannedProjects(projects.filter((project) => !ignoredProjects.includes(project.path)));
    } catch (error) {
      console.error('扫描项目失败:', error);
    } finally {
      setIsScanning(false);
    }
  }, [ignoredProjects, projectsRootDir]);

  useEffect(() => {
    void scanProjects();
  }, [scanProjects]);

  const projectDatePrefix = getProjectDatePrefix();
  const trimmedProjectName = newProjectName.trim();
  const finalProjectName = trimmedProjectName
    ? `${projectDatePrefix}${trimmedProjectName}`
    : projectDatePrefix;
  const canCreateProject = trimmedProjectName.length > 0;

  const commandHandlers = useMemo<ContributionCommandHandlers>(() => ({
    [COMMAND_CONTRIBUTIONS.selectProjectRoot.id]: async () => {
      const selected = await open({
        directory: true,
        multiple: false,
        title: '选择项目根目录',
      });
      if (selected && typeof selected === 'string') {
        await setProjectsRootDir(selected);
      }
    },
    [COMMAND_CONTRIBUTIONS.clearProjectRoot.id]: async () => {
      await setProjectsRootDir(null);
      setScannedProjects([]);
    },
    [COMMAND_CONTRIBUTIONS.createProject.id]: () => {
      setNewProjectName('');
      setShowCreateDialog(true);
    },
    [COMMAND_CONTRIBUTIONS.importProject.id]: async () => {
      const selected = await open({
        directory: true,
        multiple: false,
        title: '手动导入单个项目',
      });
      if (selected && typeof selected === 'string') {
        await onOpenProject(selected);
      }
    },
    [COMMAND_CONTRIBUTIONS.openProject.id]: async (payload) => {
      const path = payloadText(payload, 'path');
      if (!path) {
        throw new Error('打开项目命令缺少 path');
      }
      await onOpenProject(path);
    },
    [COMMAND_CONTRIBUTIONS.ignoreProject.id]: (payload) => {
      const path = payloadText(payload, 'path');
      const name = payloadText(payload, 'name') || path;
      if (!path) {
        throw new Error('忽略项目命令缺少 path');
      }
      setConfirmDialog({
        isOpen: true,
        title: '忽略项目',
        message: `忽略“${name}”？\n被忽略的项目将不再显示在项目列表中，除非手动导入。`,
        onConfirm: async () => {
          await ignoreProject(path);
          await scanProjects();
        },
      });
    },
    [COMMAND_CONTRIBUTIONS.restoreIgnoredProject.id]: async (payload) => {
      const path = payloadText(payload, 'path');
      if (!path) {
        throw new Error('恢复忽略项目命令缺少 path');
      }
      await unignoreProject(path);
    },
    [COMMAND_CONTRIBUTIONS.showIgnoredProjects.id]: () => setShowIgnoredList(true),
    [COMMAND_CONTRIBUTIONS.removeRecentProject.id]: async (payload) => {
      const path = payloadText(payload, 'path');
      if (!path) {
        throw new Error('移除最近项目命令缺少 path');
      }
      await removeRecentProject(path);
    },
  }), [
    ignoreProject,
    onOpenProject,
    removeRecentProject,
    scanProjects,
    setProjectsRootDir,
    unignoreProject,
  ]);

  const executeCommand = useCallback(async (commandId: string, payload?: JsonValue) => {
    try {
      await executeContributionCommand(
        contributionRegistry,
        commandHandlers,
        commandId,
        payload,
      );
    } catch (error) {
      setAlertDialog({ isOpen: true, title: '操作失败', message: String(error) });
    }
  }, [commandHandlers, contributionRegistry]);

  const dataSourceValues = useMemo<Record<string, JsonValue>>(() => {
    const values: Record<string, JsonValue> = {};
    values[DATA_SOURCE_CONTRIBUTIONS.projectDirectory.id] = {
      projectsRootDir,
      ignoredProjectCount: ignoredProjects.length,
    };
    values[DATA_SOURCE_CONTRIBUTIONS.projectQuickActions.id] = {
      hasProjectsRoot: Boolean(projectsRootDir),
      ignoredProjectCount: ignoredProjects.length,
    };
    values[DATA_SOURCE_CONTRIBUTIONS.recentProjects.id] = {
      settingsLoaded,
      projects: recentProjects.map((project) => ({
        path: project.path,
        name: project.name,
        openedAt: project.openedAt,
      })),
    };
    values[DATA_SOURCE_CONTRIBUTIONS.projectCatalog.id] = {
      settingsLoaded,
      isScanning,
      projects: scannedProjects.map((project) => ({
        path: project.path,
        name: project.name,
        hasPmCenter: project.hasPmCenter,
      })),
    };
    return values;
  }, [
    ignoredProjects.length,
    isScanning,
    projectsRootDir,
    recentProjects,
    scannedProjects,
    settingsLoaded,
  ]);

  const widgetRuntime = useMemo<ContributedWidgetRuntime>(() => ({
    dataSourceValues,
    executeCommand,
  }), [dataSourceValues, executeCommand]);

  const activeContentWidget = contentWidgets.find((widget) => widget.id === activeContentWidgetId);

  const handleCreateProject = async () => {
    if (!projectsRootDir || !canCreateProject) {
      return;
    }
    setIsCreating(true);
    try {
      const projectPath = await createProject(projectsRootDir, finalProjectName);
      setShowCreateDialog(false);
      setNewProjectName('');
      await onOpenProject(projectPath);
    } catch (error) {
      setAlertDialog({ isOpen: true, title: '创建失败', message: String(error) });
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <div className="flex flex-1 items-center justify-center overflow-auto p-8">
      <div className="w-full max-w-4xl">
        <header className="mb-8">
          <div className="flex items-start justify-between gap-4">
            <div className="flex-1 text-center">
              <img src={nexoraLogo} alt={APP_NAME} className="mx-auto mb-4 h-20 w-20 object-contain" />
              <h1 className="mb-1 text-2xl font-bold text-gray-900 dark:text-gray-100">{APP_NAME}</h1>
              <p className="text-sm text-gray-500 dark:text-gray-400">项目管理与渲染工作流工具</p>
              <div className="mt-2 flex flex-wrap items-center justify-center gap-x-2 gap-y-1 text-xs text-gray-400">
                <span className="font-medium uppercase tracking-[0.2em]">{APP_VERSION_TEXT}</span>
                <span className="hidden sm:inline" aria-hidden="true">·</span>
                <span>{APP_AUTHOR_NAME}</span>
                <span>{APP_AUTHOR_CONTACT}</span>
              </div>
            </div>
            {settingsToolAvailable ? <button
              type="button"
              onClick={() => setShowSettings(true)}
              className="flex items-center gap-2 rounded-lg border border-gray-200 bg-white px-3 py-2 text-gray-700 transition-colors hover:bg-gray-50 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700"
              title="全局设置"
            >
              <Settings className="h-4 w-4" />
              <span className="text-sm">全局设置</span>
            </button> : null}
          </div>
        </header>

        <div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
          {sidebarWidgets.length > 0 && (
            <aside className="space-y-4 lg:col-span-1">
              {sidebarWidgets.map((widget) => (
                <ContributedWidget
                  key={widget.id}
                  widgetId={widget.widgetId}
                  dataSourceId={widget.dataSourceId}
                  runtime={widgetRuntime}
                />
              ))}
            </aside>
          )}

          {contentWidgets.length > 0 && (
            <main className={`${sidebarWidgets.length > 0 ? 'lg:col-span-2' : 'lg:col-span-3'} h-[480px]`}>
              <div className="flex h-full flex-col rounded-xl border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800">
                <div className="flex shrink-0 border-b border-gray-200 dark:border-gray-700">
                  {contentWidgets.map((widget) => {
                    const definition = Object.values(WIDGET_CONTRIBUTIONS).find(
                      (candidate) => candidate.id === widget.widgetId,
                    );
                    let suffix = '';
                    if (widget.widgetId === WIDGET_CONTRIBUTIONS.recentProjects.id) {
                      suffix = ` (${recentProjects.length})`;
                    } else if (widget.widgetId === WIDGET_CONTRIBUTIONS.projectCatalog.id) {
                      suffix = ` (${scannedProjects.length})`;
                    }
                    return (
                      <button
                        key={widget.id}
                        type="button"
                        onClick={() => setActiveContentWidgetId(widget.id)}
                        className={`flex-1 px-4 py-3 text-sm font-medium transition-colors ${
                          activeContentWidgetId === widget.id
                            ? 'border-b-2 border-blue-600 text-blue-600'
                            : 'text-gray-600 hover:text-gray-900 dark:hover:text-gray-100'
                        }`}
                      >
                        {definition?.title || widget.id}{suffix}
                      </button>
                    );
                  })}
                </div>
                <div className="h-[calc(100%-49px)] overflow-y-auto p-4">
                  {activeContentWidget && (
                    <ContributedWidget
                      widgetId={activeContentWidget.widgetId}
                      dataSourceId={activeContentWidget.dataSourceId}
                      runtime={widgetRuntime}
                    />
                  )}
                </div>
              </div>
            </main>
          )}
        </div>
        <UiExtensionSlot
          targetComponentId="nexora.project-manager"
          pointId="nexora.project-manager.project-home-widgets"
          className="mt-6"
        />
      </div>

      <ConfirmDialog
        isOpen={confirmDialog.isOpen}
        onClose={() => setConfirmDialog((current) => ({ ...current, isOpen: false }))}
        onConfirm={confirmDialog.onConfirm}
        title={confirmDialog.title}
        message={confirmDialog.message}
        type="warning"
      />
      <AlertDialog
        isOpen={alertDialog.isOpen}
        onClose={() => setAlertDialog((current) => ({ ...current, isOpen: false }))}
        title={alertDialog.title}
        message={alertDialog.message}
      />
      <SettingsPanel
        isOpen={showSettings && settingsToolAvailable}
        onClose={() => setShowSettings(false)}
        defaultScope="global"
        onOpenProject={onOpenProject}
      />
      <Dialog
        isOpen={showIgnoredList}
        onClose={() => setShowIgnoredList(false)}
        title="已忽略的项目"
        size="md"
        footer={(
          <button
            type="button"
            onClick={() => setShowIgnoredList(false)}
            className="rounded-lg bg-gray-100 px-4 py-2 text-sm text-gray-700 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700"
          >
            关闭
          </button>
        )}
      >
        {ignoredProjects.length === 0 ? (
          <div className="py-8 text-center text-gray-400">
            <EyeOff className="mx-auto mb-3 h-12 w-12 opacity-50" />
            <p className="text-sm">暂无被忽略的项目</p>
          </div>
        ) : (
          <div className="max-h-[300px] space-y-2 overflow-y-auto">
            {ignoredProjects.map((path) => (
              <div key={path} className="flex items-center gap-3 rounded-lg bg-gray-50 p-3 dark:bg-gray-800">
                <Folder className="h-5 w-5 shrink-0 text-gray-400" />
                <p className="min-w-0 flex-1 truncate text-sm text-gray-700 dark:text-gray-300">{path}</p>
                <button
                  type="button"
                  onClick={() => executeCommand(COMMAND_CONTRIBUTIONS.restoreIgnoredProject.id, { path })}
                  className="rounded-lg p-1.5 text-gray-400 transition-colors hover:bg-blue-50 hover:text-blue-500 dark:hover:bg-blue-900/20"
                  title="恢复显示"
                >
                  <Eye className="h-4 w-4" />
                </button>
              </div>
            ))}
          </div>
        )}
      </Dialog>
      <Dialog
        isOpen={showCreateDialog}
        onClose={() => {
          setShowCreateDialog(false);
          setNewProjectName('');
        }}
        title="创建新项目"
        size="sm"
        footer={(
          <>
            <button
              type="button"
              onClick={() => {
                setShowCreateDialog(false);
                setNewProjectName('');
              }}
              className="rounded-lg px-4 py-2 text-sm text-gray-700 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800"
            >
              取消
            </button>
            <button
              type="button"
              onClick={handleCreateProject}
              disabled={!canCreateProject || isCreating}
              className="rounded-lg bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-700 disabled:bg-gray-300"
            >
              {isCreating ? '创建中...' : '创建'}
            </button>
          </>
        )}
      >
        <div className="space-y-4">
          <div>
            <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">项目名称</label>
            <div className="flex items-center rounded-lg border border-gray-300 bg-white focus-within:border-blue-500 focus-within:ring-2 focus-within:ring-blue-500/20 dark:border-gray-600 dark:bg-gray-800">
              <span className="select-none border-r border-gray-200 px-3 py-2 text-gray-500 dark:border-gray-700 dark:text-gray-400">
                {projectDatePrefix}
              </span>
              <input
                type="text"
                value={newProjectName}
                onChange={(event) => setNewProjectName(event.target.value)}
                placeholder="输入项目名称"
                className="w-full bg-transparent px-3 py-2 text-gray-900 outline-none dark:text-gray-100"
                onKeyDown={(event) => {
                  if (event.key === 'Enter') {
                    void handleCreateProject();
                  }
                }}
                autoFocus
              />
            </div>
            <p className="mt-2 text-xs text-gray-500 dark:text-gray-400">
              日期前缀固定为当天日期，你只需要输入后面的项目名称
            </p>
          </div>
          <div className="rounded-lg bg-gray-50 p-3 dark:bg-gray-800">
            <p className="text-xs text-gray-500 dark:text-gray-400">将在以下位置创建项目：</p>
            <p className="mt-1 break-all text-sm text-gray-900 dark:text-gray-100">
              {projectsRootDir}/{finalProjectName}
            </p>
          </div>
        </div>
      </Dialog>
    </div>
  );
}
