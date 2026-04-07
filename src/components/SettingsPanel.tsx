import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { usePluginStore } from '../stores/pluginStore';
import {
  AlertTriangle,
  Box,
  CheckCircle2,
  Download,
  ExternalLink,
  FolderOpen,
  HelpCircle,
  Plus,
  RefreshCw,
  Search,
  Settings,
  SlidersHorizontal,
  Trash2,
  Wrench,
  Puzzle,
} from 'lucide-react';
import { AlertDialog, Dialog } from './Dialog';
import { useOptionalProjectStoreShallow } from '../stores/projectStore';
import { ToolPaths, useSettingsStore } from '../stores/settingsStore';
import {
  DEFAULT_EXCLUDE_PATTERNS,
  PRESET_EXCLUDE_PATTERNS,
  getExcludeStorageKey,
  readProjectExcludePatterns,
} from '../utils/excludePatterns';
import { APP_VERSION_TEXT } from '../config/appMeta';
import type { PluginDescriptor, PluginDirectories } from '../types/plugin';

type SettingsScope = 'global' | 'project';

interface SettingsPanelProps {
  isOpen: boolean;
  onClose: () => void;
  defaultScope?: SettingsScope;
  onOpenProject?: (path: string) => Promise<void> | void;
}

interface ToolStatus {
  id: keyof ToolPaths;
  label: string;
  configuredPath: string | null;
  detectedPath: string | null;
  resolvedPath: string | null;
  source: string;
  status: 'ready' | 'missing';
  version: string | null;
  message: string | null;
}

const FFPROBE_DOWNLOAD_URL = 'https://ffmpeg.org/download.html#build-windows';

function toolSourceLabel(source: string) {
  switch (source) {
    case 'configured':
      return '手动指定';
    case 'system':
      return '自动探测';
    default:
      return '未找到';
  }
}

export function SettingsPanel({
  isOpen,
  onClose,
  defaultScope = 'global',
  onOpenProject,
}: SettingsPanelProps) {
  const {
    autoOpenLastProject,
    launchOnStartup,
    launchOnStartupAvailable,
    ignoredProjects,
    recentProjects,
    toolPaths,
    loadSettings,
    setAutoOpen,
    setLaunchOnStartup,
    clearAllRecentProjects,
    clearIgnoredProjects,
    unignoreProject,
    setToolPath,
    globalExcludePatterns,
    setGlobalExcludePatterns,
  } = useSettingsStore();
  const { isInitialized, projectPath, projectName, refresh } = useOptionalProjectStoreShallow((state) => ({
    isInitialized: state.isInitialized,
    projectPath: state.projectPath,
    projectName: state.projectName,
    refresh: state.refresh,
  }));

  const [activeScope, setActiveScope] = useState<SettingsScope>('global');
  const [globalPatterns, setGlobalPatterns] = useState<string[]>(DEFAULT_EXCLUDE_PATTERNS);
  const [projectPatterns, setProjectPatterns] = useState<string[]>([]);
  const [newPattern, setNewPattern] = useState('');
  const [showPresets, setShowPresets] = useState(false);
  const [needsProjectRefresh, setNeedsProjectRefresh] = useState(false);
  const [toolStatuses, setToolStatuses] = useState<ToolStatus[]>([]);
  const [isLoadingTools, setIsLoadingTools] = useState(false);
  const [isUpdatingLaunchOnStartup, setIsUpdatingLaunchOnStartup] = useState(false);
  const [globalTaskScriptsPath, setGlobalTaskScriptsPath] = useState<string | null>(null);
  const [pluginDescriptors, setPluginDescriptors] = useState<PluginDescriptor[]>([]);
  const [pluginDirectories, setPluginDirectories] = useState<PluginDirectories | null>(null);
  const [isLoadingPlugins, setIsLoadingPlugins] = useState(false);
  const [alertDialog, setAlertDialog] = useState({
    isOpen: false,
    title: '提示',
    message: '',
  });
  const refreshProjectPlugins = usePluginStore((state) => state.refreshProjectPlugins);
  const loadPluginDirsFromStore = usePluginStore((state) => state.loadPluginDirs);
  const togglePlugin = usePluginStore((state) => state.togglePlugin);

  const hasProjectScope = isInitialized && !!projectPath;
  const resolvedDefaultScope = hasProjectScope && defaultScope === 'project' ? 'project' : 'global';

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    void loadSettings();
    setActiveScope(resolvedDefaultScope);
    setGlobalPatterns(globalExcludePatterns);
  }, [isOpen, loadSettings, resolvedDefaultScope]);

  useEffect(() => {
    if (!isOpen) {
      setProjectPatterns(projectPath ? readProjectExcludePatterns(projectPath) : []);
      setGlobalTaskScriptsPath(null);
      setNeedsProjectRefresh(false);
      setNewPattern('');
      setShowPresets(false);
      return;
    }

    setProjectPatterns(projectPath ? readProjectExcludePatterns(projectPath) : []);
    setNeedsProjectRefresh(false);
    setNewPattern('');
    setShowPresets(false);
  }, [isOpen, projectPath]);

  useEffect(() => {
    if (!isOpen) {
      setGlobalPatterns(globalExcludePatterns);
      return;
    }

    setGlobalPatterns(globalExcludePatterns);
  }, [globalExcludePatterns, isOpen]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    void invoke<string>('get_global_task_scripts_path')
      .then((path) => setGlobalTaskScriptsPath(path))
      .catch((error) => {
        console.error('Failed to load global task scripts path:', error);
        setAlertDialog({
          isOpen: true,
          title: '读取全局脚本目录失败',
          message: String(error),
        });
      });
  }, [isOpen]);

  const loadToolStatuses = useCallback(async () => {
    setIsLoadingTools(true);
    try {
      const result = await invoke<ToolStatus[]>('inspect_tool_paths', { toolPaths });
      setToolStatuses(result);
    } catch (error) {
      console.error('Failed to inspect tool paths:', error);
      setAlertDialog({
        isOpen: true,
        title: '工具检测失败',
        message: String(error),
      });
    } finally {
      setIsLoadingTools(false);
    }
  }, [toolPaths]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    void loadToolStatuses();
  }, [isOpen, loadToolStatuses]);

  const loadPluginSection = useCallback(async () => {
    setIsLoadingPlugins(true);
    try {
      const [descriptors, directories] = await Promise.all([
        refreshProjectPlugins(projectPath),
        loadPluginDirsFromStore(projectPath),
      ]);
      setPluginDescriptors(descriptors);
      setPluginDirectories(directories);
    } catch (error) {
      console.error('Failed to load plugins:', error);
      setAlertDialog({
        isOpen: true,
        title: '读取插件失败',
        message: String(error),
      });
    } finally {
      setIsLoadingPlugins(false);
    }
  }, [loadPluginDirsFromStore, projectPath, refreshProjectPlugins]);

  useEffect(() => {
    if (!isOpen) {
      setPluginDescriptors([]);
      setPluginDirectories(null);
      setIsLoadingPlugins(false);
      return;
    }

    void loadPluginSection();
  }, [isOpen, loadPluginSection]);

  const sortedRecentProjects = useMemo(
    () => [...recentProjects].sort((left, right) => right.openedAt - left.openedAt),
    [recentProjects],
  );

  const currentPatterns = activeScope === 'global' ? globalPatterns : projectPatterns;

  const saveGlobalPatterns = async (patterns: string[]) => {
    setGlobalPatterns(patterns);
    await setGlobalExcludePatterns(patterns);
    setNeedsProjectRefresh(true);
  };

  const saveProjectPatterns = (patterns: string[]) => {
    setProjectPatterns(patterns);

    if (!projectPath) {
      return;
    }

    localStorage.setItem(getExcludeStorageKey(projectPath), JSON.stringify(patterns));
    setNeedsProjectRefresh(true);
  };

  const addPattern = (pattern: string) => {
    const normalized = pattern.trim();
    if (!normalized || currentPatterns.includes(normalized)) {
      setNewPattern('');
      return;
    }

    if (activeScope === 'global') {
      void saveGlobalPatterns([...globalPatterns, normalized]);
    } else {
      saveProjectPatterns([...projectPatterns, normalized]);
    }
    setNewPattern('');
    setShowPresets(false);
  };

  const removePattern = (pattern: string) => {
    if (activeScope === 'global') {
      void saveGlobalPatterns(globalPatterns.filter((item) => item !== pattern));
    } else {
      saveProjectPatterns(projectPatterns.filter((item) => item !== pattern));
    }
  };

  const handleClosePanel = async () => {
    if (needsProjectRefresh && isInitialized) {
      try {
        await refresh();
      } catch (error) {
        console.error('Failed to refresh after settings change:', error);
      }
    }

    onClose();
  };

  const handleSelectToolPath = async (tool: keyof ToolPaths) => {
    try {
      const selected = await open({
        directory: false,
        multiple: false,
        title: tool === 'ffprobe' ? '选择 ffprobe 可执行文件' : '选择 Blender 可执行文件',
        filters: [
          {
            name: 'Executable',
            extensions: ['exe'],
          },
        ],
      });

      if (selected && typeof selected === 'string') {
        await setToolPath(tool, selected);
        await loadToolStatuses();
      }
    } catch (error) {
      setAlertDialog({
        isOpen: true,
        title: '选择工具路径失败',
        message: String(error),
      });
    }
  };

  const handleClearToolPath = async (tool: keyof ToolPaths) => {
    await setToolPath(tool, null);
    await loadToolStatuses();
  };

  const handleToggleLaunchOnStartup = async (enabled: boolean) => {
    setIsUpdatingLaunchOnStartup(true);

    try {
      await setLaunchOnStartup(enabled);
    } catch (error) {
      setAlertDialog({
        isOpen: true,
        title: enabled ? '启用开机自启动失败' : '关闭开机自启动失败',
        message: String(error),
      });
    } finally {
      setIsUpdatingLaunchOnStartup(false);
    }
  };

  const handleOpenFfprobeDownloadPage = async () => {
    try {
      await openUrl(FFPROBE_DOWNLOAD_URL);
    } catch (error) {
      setAlertDialog({
        isOpen: true,
        title: '打开下载页失败',
        message: String(error),
      });
    }
  };

  const handleOpenGlobalTaskScriptsDir = async () => {
    if (!globalTaskScriptsPath) {
      return;
    }

    try {
      await invoke('open_path', { path: globalTaskScriptsPath });
    } catch (error) {
      setAlertDialog({
        isOpen: true,
        title: '打开全局脚本目录失败',
        message: String(error),
      });
    }
  };

  const handleOpenPluginDirectory = async (targetPath?: string | null) => {
    if (!targetPath) {
      return;
    }

    try {
      await invoke('open_path', { path: targetPath });
    } catch (error) {
      setAlertDialog({
        isOpen: true,
        title: '打开插件目录失败',
        message: String(error),
      });
    }
  };

  const handleOpenDirectoryAsProject = async (targetPath?: string | null) => {
    if (!targetPath || !onOpenProject) {
      return;
    }

    try {
      await Promise.resolve(onOpenProject(targetPath));
      onClose();
    } catch (error) {
      setAlertDialog({
        isOpen: true,
        title: '作为项目打开失败',
        message: String(error),
      });
    }
  };

  const handleTogglePlugin = async (plugin: PluginDescriptor, enabled: boolean) => {
    try {
      await togglePlugin(projectPath, plugin.key, enabled);
      await loadPluginSection();
    } catch (error) {
      setAlertDialog({
        isOpen: true,
        title: enabled ? '启用插件失败' : '禁用插件失败',
        message: String(error),
      });
    }
  };

  const globalPlugins = useMemo(
    () => pluginDescriptors.filter((plugin) => plugin.scope === 'global'),
    [pluginDescriptors],
  );
  const projectPlugins = useMemo(
    () => pluginDescriptors.filter((plugin) => plugin.scope === 'project'),
    [pluginDescriptors],
  );

  const renderDirectoryActionButtons = ({
    targetPath,
    explorerAction,
    projectAction,
  }: {
    targetPath?: string | null;
    explorerAction: () => void;
    projectAction: () => void;
  }) => (
    <div className="flex flex-wrap items-center justify-end gap-2">
      <button
        onClick={explorerAction}
        disabled={!targetPath}
        className="px-3 py-2 text-sm rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 hover:bg-gray-100 dark:hover:bg-gray-800 disabled:bg-gray-100 disabled:text-gray-400 disabled:border-gray-200 text-gray-700 dark:text-gray-200 transition-colors flex items-center gap-1.5"
      >
        <ExternalLink className="w-4 h-4" />
        系统文件夹
      </button>
      <button
        onClick={projectAction}
        disabled={!targetPath || !onOpenProject}
        className="px-3 py-2 text-sm rounded-lg bg-blue-600 hover:bg-blue-700 disabled:bg-gray-300 text-white transition-colors flex items-center gap-1.5"
      >
        <FolderOpen className="w-4 h-4" />
        作为项目打开
      </button>
    </div>
  );

  const renderExcludeRulesSection = ({
    title,
    description,
    patterns,
    emptyText,
    onClear,
  }: {
    title: string;
    description: string;
    patterns: string[];
    emptyText: string;
    onClear: () => void;
  }) => (
    <section className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
      <div className="flex items-start gap-2 mb-4 rounded-lg bg-blue-50 dark:bg-blue-900/20 px-3 py-3">
        <HelpCircle className="w-5 h-5 text-blue-500 flex-shrink-0 mt-0.5" />
        <div className="text-sm text-blue-700 dark:text-blue-300">
          <p className="font-medium">{title}</p>
          <p className="mt-1 text-xs text-blue-600 dark:text-blue-400">{description}</p>
        </div>
      </div>

      <div className="mb-4">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
          添加排除规则
        </label>
        <div className="flex gap-2">
          <input
            type="text"
            value={newPattern}
            onChange={(event) => setNewPattern(event.target.value)}
            placeholder="例如: *.log 或 cache"
            className="flex-1 px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
                       bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
                       focus:outline-none focus:ring-2 focus:ring-blue-500"
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                addPattern(newPattern);
              }
            }}
          />
          <button
            onClick={() => addPattern(newPattern)}
            disabled={!newPattern.trim()}
            className="px-4 py-2 rounded-lg bg-blue-600 hover:bg-blue-700 disabled:bg-gray-300 text-white transition-colors"
          >
            <Plus className="w-4 h-4" />
          </button>
        </div>

        <div className="mt-2">
          <button
            onClick={() => setShowPresets((value) => !value)}
            className="text-sm text-blue-600 dark:text-blue-400 hover:underline"
          >
            {showPresets ? '隐藏预设规则' : '从预设选择...'}
          </button>

          {showPresets && (
            <div className="mt-2 space-y-1 rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 p-2">
              {PRESET_EXCLUDE_PATTERNS.map((preset) => (
                <button
                  key={preset.value}
                  onClick={() => addPattern(preset.value)}
                  disabled={patterns.includes(preset.value)}
                  className="w-full flex items-center justify-between gap-3 rounded-md px-3 py-2 text-left text-sm hover:bg-gray-100 dark:hover:bg-gray-700 disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  <div>
                    <span className="font-medium text-gray-700 dark:text-gray-200">{preset.label}</span>
                    <span className="ml-2 text-xs text-gray-400">{preset.desc}</span>
                  </div>
                  {patterns.includes(preset.value) ? (
                    <span className="text-xs text-green-600">已添加</span>
                  ) : (
                    <Plus className="w-4 h-4 text-gray-400" />
                  )}
                </button>
              ))}
            </div>
          )}
        </div>
      </div>

      <div>
        <div className="flex items-center justify-between mb-2">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            当前排除规则 ({patterns.length})
          </label>
          {patterns.length > 0 && (
            <button
              onClick={onClear}
              className="text-xs text-red-600 hover:text-red-700"
            >
              清空全部
            </button>
          )}
        </div>

        {patterns.length === 0 ? (
          <div className="flex items-center justify-center py-8 text-gray-400">
            <div className="text-center">
              <FolderOpen className="w-8 h-8 mx-auto mb-2 opacity-50" />
              <p className="text-sm">{emptyText}</p>
            </div>
          </div>
        ) : (
          <div className="space-y-1 max-h-64 overflow-auto">
            {patterns.map((pattern) => (
              <div
                key={pattern}
                className="flex items-center justify-between gap-3 rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 px-3 py-2"
              >
                <code className="text-sm font-mono text-gray-700 dark:text-gray-300">{pattern}</code>
                <button
                  onClick={() => removePattern(pattern)}
                  className="p-1 text-gray-400 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20 rounded transition-colors"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );

  const renderPluginSection = ({
    title,
    description,
    directoryPath,
    plugins,
  }: {
    title: string;
    description: string;
    directoryPath?: string | null;
    plugins: PluginDescriptor[];
  }) => (
    <section className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
      <div className="flex items-center gap-2 mb-3">
        <Puzzle className="w-4 h-4 text-blue-500" />
        <div className="flex-1">
          <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">{title}</h4>
          <p className="text-xs text-gray-500 mt-1">{description}</p>
        </div>
        <button
          onClick={() => void loadPluginSection()}
          className="p-2 rounded-lg text-gray-500 hover:text-gray-700 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
          title="刷新插件"
        >
          <RefreshCw className={`w-4 h-4 ${isLoadingPlugins ? 'animate-spin' : ''}`} />
        </button>
      </div>

      <div className="rounded-lg bg-gray-50 dark:bg-gray-800 px-3 py-3 mb-3">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="text-xs text-gray-500 mb-1">目录位置</p>
            <p className="text-sm text-gray-900 dark:text-gray-100 break-all">
              {directoryPath || '当前范围没有插件目录'}
            </p>
          </div>
          {renderDirectoryActionButtons({
            targetPath: directoryPath,
            explorerAction: () => void handleOpenPluginDirectory(directoryPath),
            projectAction: () => void handleOpenDirectoryAsProject(directoryPath),
          })}
        </div>
      </div>

      {activeScope === 'global' && pluginDirectories?.runtime && (
        <div className="mb-3 rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 px-3 py-3">
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <p className="text-sm font-medium text-gray-900 dark:text-gray-100">插件 Python 运行时</p>
              <p className="mt-1 text-xs text-gray-500">
                状态: {pluginDirectories.runtime.status} · 来源: {pluginDirectories.runtime.source}
                {pluginDirectories.runtime.version ? ` · ${pluginDirectories.runtime.version}` : ''}
              </p>
              <p className="mt-2 text-xs text-gray-700 dark:text-gray-300 break-all">
                {pluginDirectories.runtime.resolvedPath || pluginDirectories.runtime.message || '未找到运行时'}
              </p>
            </div>
          </div>
        </div>
      )}

      {plugins.length === 0 ? (
        <div className="rounded-lg bg-gray-50 dark:bg-gray-800 px-3 py-6 text-sm text-gray-500 dark:text-gray-400">
          当前范围还没有检测到插件。
        </div>
      ) : (
        <div className="space-y-2">
          {plugins.map((plugin) => (
            <div
              key={plugin.key}
              className="rounded-xl border border-gray-200 dark:border-gray-700 bg-gray-50/80 dark:bg-gray-800/60 p-4"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-sm text-gray-900 dark:text-gray-100">{plugin.name}</span>
                    <span className="px-2 py-0.5 text-xs rounded-full bg-blue-100 text-blue-700 dark:bg-blue-900/20 dark:text-blue-300">
                      {plugin.scope === 'project' ? '项目' : '全局'}
                    </span>
                    {plugin.shadowedBy ? (
                      <span className="px-2 py-0.5 text-xs rounded-full bg-amber-100 text-amber-700 dark:bg-amber-900/20 dark:text-amber-300">
                        被 {plugin.shadowedBy} 覆盖
                      </span>
                    ) : null}
                  </div>
                  <p className="mt-1 text-xs text-gray-500">
                    {plugin.id} · v{plugin.version} · {plugin.runtime}
                  </p>
                  <p className="mt-2 text-sm text-gray-800 dark:text-gray-200 break-all">
                    {plugin.path}
                  </p>
                  {plugin.description ? (
                    <p className="mt-2 text-xs text-gray-500 dark:text-gray-400">{plugin.description}</p>
                  ) : null}
                  {plugin.validationIssues.length > 0 ? (
                    <div className="mt-2 space-y-1 text-xs text-red-600 dark:text-red-400">
                      {plugin.validationIssues.map((issue) => (
                        <div key={`${plugin.key}-${issue.code}`}>{issue.message}</div>
                      ))}
                    </div>
                  ) : (
                    <p className="mt-2 text-xs text-green-600 dark:text-green-400">
                      动作数: {plugin.actions.length}
                    </p>
                  )}
                </div>

                <label className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-300">
                  <input
                    type="checkbox"
                    checked={plugin.enabled}
                    disabled={plugin.validationIssues.length > 0 || Boolean(plugin.shadowedBy)}
                    onChange={(event) => void handleTogglePlugin(plugin, event.target.checked)}
                    className="rounded"
                  />
                  启用
                </label>
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );

  const renderGlobalSettings = () => (
    <div className="space-y-4">
      <section className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
        <div className="flex items-center gap-2 mb-3">
          <SlidersHorizontal className="w-4 h-4 text-blue-500" />
          <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">常规</h4>
        </div>
        <div className="space-y-4">
          <label className="flex items-start gap-3 text-sm text-gray-700 dark:text-gray-300">
            <input
              type="checkbox"
              checked={autoOpenLastProject}
              onChange={(event) => void setAutoOpen(event.target.checked)}
              className="mt-0.5 rounded"
            />
            <span>
              启动后自动打开上次项目
              <span className="block text-xs text-gray-500 mt-1">
                关闭后将始终先进入主页。
              </span>
            </span>
          </label>

          <label
            className={`flex items-start gap-3 text-sm text-gray-700 dark:text-gray-300 ${
              !launchOnStartupAvailable ? 'opacity-60' : ''
            }`}
          >
            <input
              type="checkbox"
              checked={launchOnStartup}
              disabled={!launchOnStartupAvailable || isUpdatingLaunchOnStartup}
              onChange={(event) => void handleToggleLaunchOnStartup(event.target.checked)}
              className="mt-0.5 rounded"
            />
            <span>
              开机自动启动
              <span className="block text-xs text-gray-500 mt-1">
                {launchOnStartupAvailable
                  ? isUpdatingLaunchOnStartup
                    ? '正在更新系统启动项...'
                    : '在系统登录后自动启动 PM Center。'
                  : '当前环境暂时无法读取系统自启动状态。'}
              </span>
            </span>
          </label>
        </div>
      </section>

      {renderExcludeRulesSection({
        title: '全局排除规则',
        description: '对所有项目统一生效。默认已包含 .blend1/.blend2/... 这类 Blender 备份文件规则。',
        patterns: globalPatterns,
        emptyText: '暂无全局排除规则',
        onClear: () => {
          if (confirm('确定要清空所有全局排除规则吗？')) {
            void saveGlobalPatterns([]);
          }
        },
      })}

      <section className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
        <div className="flex items-center gap-2 mb-3">
          <FolderOpen className="w-4 h-4 text-blue-500" />
          <div className="flex-1">
            <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">全局任务脚本目录</h4>
            <p className="text-xs text-gray-500 mt-1">这里存放所有项目共用的通用任务脚本。</p>
          </div>
          {renderDirectoryActionButtons({
            targetPath: globalTaskScriptsPath,
            explorerAction: () => void handleOpenGlobalTaskScriptsDir(),
            projectAction: () => void handleOpenDirectoryAsProject(globalTaskScriptsPath),
          })}
        </div>

        <div className="rounded-lg bg-gray-50 dark:bg-gray-800 px-3 py-3">
          <p className="text-xs text-gray-500 mb-1">目录位置</p>
          <p className="text-sm text-gray-900 dark:text-gray-100 break-all">
            {globalTaskScriptsPath || '读取中...'}
          </p>
        </div>
      </section>

      {renderPluginSection({
        title: '全局插件',
        description: '扫描应用级插件目录，供所有项目使用。项目内同 id 插件会覆盖这里的全局插件。',
        directoryPath: pluginDirectories?.globalPath,
        plugins: globalPlugins,
      })}

      <section className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
        <div className="flex items-center gap-2 mb-3">
          <HelpCircle className="w-4 h-4 text-blue-500" />
          <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">主页入口说明</h4>
        </div>
        <div className="rounded-lg bg-blue-50 dark:bg-blue-900/20 px-3 py-3 text-sm text-blue-700 dark:text-blue-300">
          <p>项目根目录的选择和更换已经保留在主页左侧卡片里。</p>
          <p className="mt-2 text-xs text-blue-600 dark:text-blue-400">
            这里的全局设置只保留通用偏好和工具路径，不再承载项目根目录选择。
          </p>
        </div>
      </section>

      <section className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
        <div className="flex items-center gap-2 mb-3">
          <Wrench className="w-4 h-4 text-blue-500" />
          <div className="flex-1">
            <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">工具路径</h4>
            <p className="text-xs text-gray-500 mt-1">视频分析依赖 `ffprobe`，`.blend` 优先使用内置 BlendIO，`Blender` 仅用于兼容回退。</p>
          </div>
          <button
            onClick={() => void loadToolStatuses()}
            className="p-2 rounded-lg text-gray-500 hover:text-gray-700 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
            title="重新检测工具"
          >
            <RefreshCw className={`w-4 h-4 ${isLoadingTools ? 'animate-spin' : ''}`} />
          </button>
        </div>

        <div className="space-y-3">
          {toolStatuses.map((tool) => (
            <div
              key={tool.id}
              className="rounded-xl border border-gray-200 dark:border-gray-700 bg-gray-50/80 dark:bg-gray-800/60 p-4"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    {tool.id === 'blender' ? (
                      <Box className="w-4 h-4 text-orange-500" />
                    ) : (
                      <Search className="w-4 h-4 text-emerald-500" />
                    )}
                    <span className="font-medium text-sm text-gray-900 dark:text-gray-100">{tool.label}</span>
                    <span
                      className={`px-2 py-0.5 text-xs rounded-full ${
                        tool.status === 'ready'
                          ? 'bg-green-100 text-green-700 dark:bg-green-900/20 dark:text-green-400'
                          : 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/20 dark:text-yellow-400'
                      }`}
                    >
                      {tool.status === 'ready' ? '可用' : '缺失'}
                    </span>
                  </div>
                  <p className="mt-2 text-xs text-gray-500">
                    来源：{toolSourceLabel(tool.source)}
                    {tool.version ? ` · ${tool.version}` : ''}
                  </p>
                  <p className="mt-2 text-sm text-gray-800 dark:text-gray-200 break-all">
                    {tool.resolvedPath || '当前没有可用路径'}
                  </p>
                  {tool.message && (
                    <div className="mt-2 flex items-start gap-2 text-xs text-gray-500">
                      {tool.status === 'ready' ? (
                        <CheckCircle2 className="w-4 h-4 text-green-500 flex-shrink-0 mt-0.5" />
                      ) : (
                        <AlertTriangle className="w-4 h-4 text-yellow-500 flex-shrink-0 mt-0.5" />
                      )}
                      <span>{tool.message}</span>
                    </div>
                  )}
                </div>

                <div className="flex flex-col gap-2">
                  <button
                    onClick={() => void handleSelectToolPath(tool.id)}
                    className="px-3 py-2 text-sm rounded-lg bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                  >
                    指定路径
                  </button>

                  {tool.id === 'ffprobe' && (
                    <button
                      onClick={() => void handleOpenFfprobeDownloadPage()}
                      className="px-3 py-2 text-sm rounded-lg bg-blue-600 hover:bg-blue-700 text-white transition-colors flex items-center justify-center gap-1.5"
                    >
                      <Download className="w-4 h-4" />
                      打开下载页
                    </button>
                  )}

                  {tool.configuredPath && (
                    <button
                      onClick={() => void handleClearToolPath(tool.id)}
                      className="px-3 py-2 text-sm rounded-lg text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors"
                    >
                      清除指定
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}
        </div>
      </section>

      <section className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
        <div className="flex items-center gap-2 mb-3">
          <Settings className="w-4 h-4 text-blue-500" />
          <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">历史与忽略列表</h4>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="rounded-lg bg-gray-50 dark:bg-gray-800 p-3">
            <div className="flex items-center justify-between gap-2">
              <div>
                <p className="text-sm font-medium text-gray-900 dark:text-gray-100">最近项目</p>
                <p className="text-xs text-gray-500 mt-1">共 {sortedRecentProjects.length} 条记录</p>
              </div>
              {sortedRecentProjects.length > 0 && (
                <button
                  onClick={() => {
                    if (confirm('确定要清空最近项目记录吗？')) {
                      void clearAllRecentProjects();
                    }
                  }}
                  className="p-2 text-gray-400 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors"
                  title="清空最近项目"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              )}
            </div>
            <div className="mt-3 space-y-2 max-h-40 overflow-auto">
              {sortedRecentProjects.length === 0 ? (
                <p className="text-xs text-gray-400">暂无最近项目</p>
              ) : (
                sortedRecentProjects.slice(0, 6).map((project) => (
                  <div key={project.path} className="rounded-md bg-white dark:bg-gray-900 px-3 py-2 border border-gray-200 dark:border-gray-700">
                    <p className="text-sm text-gray-900 dark:text-gray-100 truncate">{project.name}</p>
                    <p className="text-xs text-gray-500 mt-1 truncate">{project.path}</p>
                  </div>
                ))
              )}
            </div>
          </div>

          <div className="rounded-lg bg-gray-50 dark:bg-gray-800 p-3">
            <div className="flex items-center justify-between gap-2">
              <div>
                <p className="text-sm font-medium text-gray-900 dark:text-gray-100">已忽略项目</p>
                <p className="text-xs text-gray-500 mt-1">共 {ignoredProjects.length} 条记录</p>
              </div>
              {ignoredProjects.length > 0 && (
                <button
                  onClick={() => {
                    if (confirm('确定要清空已忽略项目列表吗？')) {
                      void clearIgnoredProjects();
                    }
                  }}
                  className="p-2 text-gray-400 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors"
                  title="清空忽略列表"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              )}
            </div>
            <div className="mt-3 space-y-2 max-h-40 overflow-auto">
              {ignoredProjects.length === 0 ? (
                <p className="text-xs text-gray-400">暂无已忽略项目</p>
              ) : (
                ignoredProjects.map((path) => (
                  <div
                    key={path}
                    className="rounded-md bg-white dark:bg-gray-900 px-3 py-2 border border-gray-200 dark:border-gray-700 flex items-center gap-2"
                  >
                    <div className="min-w-0 flex-1">
                      <p className="text-xs text-gray-600 dark:text-gray-300 break-all">{path}</p>
                    </div>
                    <button
                      onClick={() => void unignoreProject(path)}
                      className="px-2 py-1 text-xs rounded-md bg-blue-50 hover:bg-blue-100 dark:bg-blue-900/20 dark:hover:bg-blue-900/30 text-blue-600 dark:text-blue-400 transition-colors"
                    >
                      恢复
                    </button>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </section>
    </div>
  );

  const renderProjectSettings = () => (
    <div className="space-y-4">
      {renderExcludeRulesSection({
        title: '项目排除规则',
        description: `当前项目：${projectName || '未打开项目'}。这里只追加项目专属规则；全局规则会继续一起生效。`,
        patterns: projectPatterns,
        emptyText: '暂无项目专属排除规则',
        onClear: () => {
          if (confirm('确定要清空当前项目的所有排除规则吗？')) {
            saveProjectPatterns([]);
          }
        },
      })}

      {projectPath && (
        <div className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
          <p className="text-sm font-medium text-gray-900 dark:text-gray-100">当前项目路径</p>
          <p className="mt-2 text-xs text-gray-500 break-all">{projectPath}</p>
          <p className="mt-3 text-xs text-blue-600 dark:text-blue-400">
            项目排除规则会叠加到全局规则上，一起影响文件列表显示、搜索和刷新结果。
          </p>
        </div>
      )}

      {renderPluginSection({
        title: '项目插件',
        description: `当前项目：${projectName || '未打开项目'}。这里的插件只对当前项目生效，并且会覆盖同 id 的全局插件。`,
        directoryPath: pluginDirectories?.projectPath,
        plugins: projectPlugins,
      })}

      {needsProjectRefresh && (
        <div className="flex items-start gap-2 rounded-lg border border-yellow-200 bg-yellow-50 px-3 py-2 text-sm text-yellow-700">
          <AlertTriangle className="w-4 h-4 flex-shrink-0 mt-0.5" />
          <span>排除规则已更新，关闭设置后会自动刷新当前项目。</span>
        </div>
      )}
    </div>
  );

  return (
    <>
      <Dialog
        isOpen={isOpen}
        onClose={() => void handleClosePanel()}
        title={`设置 · ${APP_VERSION_TEXT}`}
        size="xl"
        footer={
          <button
            onClick={() => void handleClosePanel()}
            className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded-lg transition-colors"
          >
            关闭
          </button>
        }
      >
        <div className="space-y-4">
          <div className="flex items-center gap-2 rounded-xl bg-gray-100 dark:bg-gray-800 p-1">
            <button
              onClick={() => setActiveScope('global')}
              className={`flex-1 rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                activeScope === 'global'
                  ? 'bg-white dark:bg-gray-900 text-blue-600 shadow-sm'
                  : 'text-gray-600 dark:text-gray-300 hover:text-gray-900 dark:hover:text-gray-100'
              }`}
            >
              全局设置
            </button>
            <button
              onClick={() => hasProjectScope && setActiveScope('project')}
              disabled={!hasProjectScope}
              className={`flex-1 rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                activeScope === 'project'
                  ? 'bg-white dark:bg-gray-900 text-blue-600 shadow-sm'
                  : 'text-gray-600 dark:text-gray-300 hover:text-gray-900 dark:hover:text-gray-100'
              } disabled:opacity-50 disabled:cursor-not-allowed`}
            >
              项目设置
            </button>
          </div>

          {activeScope === 'global' ? renderGlobalSettings() : renderProjectSettings()}
        </div>
      </Dialog>

      <AlertDialog
        isOpen={alertDialog.isOpen}
        onClose={() => setAlertDialog((state) => ({ ...state, isOpen: false }))}
        title={alertDialog.title}
        message={alertDialog.message}
      />
    </>
  );
}
