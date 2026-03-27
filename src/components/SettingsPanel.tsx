import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  AlertTriangle,
  Box,
  CheckCircle2,
  Download,
  FolderOpen,
  HelpCircle,
  Plus,
  RefreshCw,
  Search,
  Settings,
  SlidersHorizontal,
  Trash2,
  Wrench,
} from 'lucide-react';
import { AlertDialog, Dialog } from './Dialog';
import { useProjectStore } from '../stores/projectStore';
import { ToolPaths, useSettingsStore } from '../stores/settingsStore';

type SettingsScope = 'global' | 'project';

interface SettingsPanelProps {
  isOpen: boolean;
  onClose: () => void;
  defaultScope?: SettingsScope;
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

const DEFAULT_EXCLUDE_PATTERNS = ['.pm_center', '.git', '*.tmp', '*.temp', 'Thumbs.db', '.DS_Store'];
const FFPROBE_DOWNLOAD_URL = 'https://ffmpeg.org/download.html#build-windows';
const PRESET_PATTERNS = [
  { value: '.git', label: 'Git 目录 (.git)', desc: '版本控制目录' },
  { value: 'node_modules', label: 'Node 模块 (node_modules)', desc: '依赖目录' },
  { value: '__pycache__', label: 'Python 缓存 (__pycache__)', desc: '编译缓存' },
  { value: '*.tmp', label: '临时文件 (*.tmp)', desc: '临时文件' },
  { value: '*.bak', label: '备份文件 (*.bak)', desc: '备份文件' },
  { value: '.DS_Store', label: 'Mac 索引 (.DS_Store)', desc: '系统文件' },
  { value: 'Thumbs.db', label: 'Windows 缩略图 (Thumbs.db)', desc: '系统文件' },
];

function getExcludeStorageKey(projectPath: string) {
  return `project_exclude_${projectPath}`;
}

function readExcludePatterns(projectPath: string | null) {
  if (!projectPath) {
    return [...DEFAULT_EXCLUDE_PATTERNS];
  }

  const saved = localStorage.getItem(getExcludeStorageKey(projectPath));
  if (!saved) {
    return [...DEFAULT_EXCLUDE_PATTERNS];
  }

  try {
    const parsed = JSON.parse(saved);
    return Array.isArray(parsed) ? parsed : [...DEFAULT_EXCLUDE_PATTERNS];
  } catch {
    return [...DEFAULT_EXCLUDE_PATTERNS];
  }
}

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
}: SettingsPanelProps) {
  const {
    autoOpenLastProject,
    ignoredProjects,
    recentProjects,
    toolPaths,
    loadSettings,
    setAutoOpen,
    clearAllRecentProjects,
    clearIgnoredProjects,
    unignoreProject,
    setToolPath,
  } = useSettingsStore();
  const { isInitialized, projectPath, projectName, refresh } = useProjectStore();

  const [activeScope, setActiveScope] = useState<SettingsScope>('global');
  const [excludePatterns, setExcludePatterns] = useState<string[]>(DEFAULT_EXCLUDE_PATTERNS);
  const [newPattern, setNewPattern] = useState('');
  const [showPresets, setShowPresets] = useState(false);
  const [needsProjectRefresh, setNeedsProjectRefresh] = useState(false);
  const [toolStatuses, setToolStatuses] = useState<ToolStatus[]>([]);
  const [isLoadingTools, setIsLoadingTools] = useState(false);
  const [alertDialog, setAlertDialog] = useState({
    isOpen: false,
    title: '提示',
    message: '',
  });

  const hasProjectScope = isInitialized && !!projectPath;
  const resolvedDefaultScope = hasProjectScope && defaultScope === 'project' ? 'project' : 'global';

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    void loadSettings();
    setActiveScope(resolvedDefaultScope);
  }, [isOpen, loadSettings, resolvedDefaultScope]);

  useEffect(() => {
    if (!isOpen || !projectPath) {
      setExcludePatterns([...DEFAULT_EXCLUDE_PATTERNS]);
      setNeedsProjectRefresh(false);
      setNewPattern('');
      setShowPresets(false);
      return;
    }

    setExcludePatterns(readExcludePatterns(projectPath));
    setNeedsProjectRefresh(false);
    setNewPattern('');
    setShowPresets(false);
  }, [isOpen, projectPath]);

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

  const sortedRecentProjects = useMemo(
    () => [...recentProjects].sort((left, right) => right.openedAt - left.openedAt),
    [recentProjects],
  );

  const saveExcludePatterns = (patterns: string[]) => {
    setExcludePatterns(patterns);

    if (!projectPath) {
      return;
    }

    localStorage.setItem(getExcludeStorageKey(projectPath), JSON.stringify(patterns));
    setNeedsProjectRefresh(true);
  };

  const addPattern = (pattern: string) => {
    const normalized = pattern.trim();
    if (!normalized || excludePatterns.includes(normalized)) {
      setNewPattern('');
      return;
    }

    saveExcludePatterns([...excludePatterns, normalized]);
    setNewPattern('');
    setShowPresets(false);
  };

  const removePattern = (pattern: string) => {
    saveExcludePatterns(excludePatterns.filter((item) => item !== pattern));
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

  const renderGlobalSettings = () => (
    <div className="space-y-4">
      <section className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
        <div className="flex items-center gap-2 mb-3">
          <SlidersHorizontal className="w-4 h-4 text-blue-500" />
          <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">常规</h4>
        </div>
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
      </section>

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
            <p className="text-xs text-gray-500 mt-1">视频分析依赖 `ffprobe`，`.blend` 深度信息依赖 `Blender`。</p>
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
      <section className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
        <div className="flex items-start gap-2 mb-4 rounded-lg bg-blue-50 dark:bg-blue-900/20 px-3 py-3">
          <HelpCircle className="w-5 h-5 text-blue-500 flex-shrink-0 mt-0.5" />
          <div className="text-sm text-blue-700 dark:text-blue-300">
            <p className="font-medium">当前项目</p>
            <p className="mt-1 text-xs">{projectName || '未打开项目'}</p>
            {projectPath && <p className="mt-1 text-xs break-all">{projectPath}</p>}
            <p className="mt-2 text-xs text-blue-600 dark:text-blue-400">
              排除规则会影响文件列表显示、搜索和刷新结果；关闭设置时会自动刷新当前项目。
            </p>
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
                {PRESET_PATTERNS.map((preset) => (
                  <button
                    key={preset.value}
                    onClick={() => addPattern(preset.value)}
                    disabled={excludePatterns.includes(preset.value)}
                    className="w-full flex items-center justify-between gap-3 rounded-md px-3 py-2 text-left text-sm hover:bg-gray-100 dark:hover:bg-gray-700 disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    <div>
                      <span className="font-medium text-gray-700 dark:text-gray-200">{preset.label}</span>
                      <span className="ml-2 text-xs text-gray-400">{preset.desc}</span>
                    </div>
                    {excludePatterns.includes(preset.value) ? (
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
              当前排除规则 ({excludePatterns.length})
            </label>
            {excludePatterns.length > 0 && (
              <button
                onClick={() => {
                  if (confirm('确定要清空当前项目的所有排除规则吗？')) {
                    saveExcludePatterns([]);
                  }
                }}
                className="text-xs text-red-600 hover:text-red-700"
              >
                清空全部
              </button>
            )}
          </div>

          {excludePatterns.length === 0 ? (
            <div className="flex items-center justify-center py-8 text-gray-400">
              <div className="text-center">
                <FolderOpen className="w-8 h-8 mx-auto mb-2 opacity-50" />
                <p className="text-sm">暂无排除规则</p>
              </div>
            </div>
          ) : (
            <div className="space-y-1 max-h-64 overflow-auto">
              {excludePatterns.map((pattern) => (
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

          {needsProjectRefresh && (
            <div className="mt-3 flex items-start gap-2 rounded-lg border border-yellow-200 bg-yellow-50 px-3 py-2 text-sm text-yellow-700">
              <AlertTriangle className="w-4 h-4 flex-shrink-0 mt-0.5" />
              <span>排除规则已更新，关闭设置后会自动刷新当前项目。</span>
            </div>
          )}
        </div>
      </section>
    </div>
  );

  return (
    <>
      <Dialog
        isOpen={isOpen}
        onClose={() => void handleClosePanel()}
        title="设置"
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
