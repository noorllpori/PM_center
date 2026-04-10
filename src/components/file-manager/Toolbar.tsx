import { useEffect, useMemo, useRef, useState } from 'react';
import type { FileInfo } from '../../types';
import { useProjectStoreShallow } from '../../stores/projectStore';
import { usePluginStore } from '../../stores/pluginStore';
import { useTaskStore } from '../../stores/taskStore';
import { useUiStore } from '../../stores/uiStore';
import { SettingsPanel } from '../SettingsPanel';
import { APP_VERSION } from '../../config/appMeta';
import { buildPluginContextItems, getVisiblePluginActions } from '../../utils/pluginActions';
import {
  List,
  Grid,
  Search,
  X,
  Settings,
  Puzzle,
  ChevronDown,
} from 'lucide-react';

export const TOOLBAR_SEARCH_FOCUS_EVENT = 'pm-center:focus-toolbar-search';

interface ToolbarProps {
  onOpenProject: (path: string) => Promise<void> | void;
}

export function Toolbar({ onOpenProject }: ToolbarProps) {
  const {
    viewMode,
    setViewMode,
    currentPath,
    projectPath,
    projectName,
    files,
    searchResults,
    selectedFiles,
    search,
    searchQuery,
    clearSearch,
    isInitialized,
  } = useProjectStoreShallow((state) => ({
    viewMode: state.viewMode,
    setViewMode: state.setViewMode,
    currentPath: state.currentPath,
    projectPath: state.projectPath,
    projectName: state.projectName,
    files: state.files,
    searchResults: state.searchResults,
    selectedFiles: state.selectedFiles,
    search: state.search,
    searchQuery: state.searchQuery,
    clearSearch: state.clearSearch,
    isInitialized: state.isInitialized,
  }));

  const [isSearchOpen, setIsSearchOpen] = useState(false);
  const [localSearch, setLocalSearch] = useState('');
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isPluginMenuOpen, setIsPluginMenuOpen] = useState(false);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const pluginMenuRef = useRef<HTMLDivElement | null>(null);
  const addTask = useTaskStore((state) => state.addTask);
  const showToast = useUiStore((state) => state.showToast);
  const pluginProjectKey = projectPath || '__global__';
  const pluginState = usePluginStore((state) => state.byProject[pluginProjectKey]);
  const loadPlugins = usePluginStore((state) => state.loadPlugins);
  const refreshProjectPlugins = usePluginStore((state) => state.refreshProjectPlugins);

  useEffect(() => {
    setLocalSearch(searchQuery);
  }, [searchQuery]);

  useEffect(() => {
    setLocalSearch(searchQuery);
    setIsSearchOpen(Boolean(searchQuery));
  }, [projectPath]);

  useEffect(() => {
    if (!isSearchOpen) {
      return;
    }

    window.requestAnimationFrame(() => {
      searchInputRef.current?.focus();
      searchInputRef.current?.select();
    });
  }, [isSearchOpen]);

  useEffect(() => {
    const handleFocusSearch = () => {
      if (!isInitialized) {
        return;
      }

      setIsSearchOpen(true);
    };

    window.addEventListener(TOOLBAR_SEARCH_FOCUS_EVENT, handleFocusSearch);
    return () => window.removeEventListener(TOOLBAR_SEARCH_FOCUS_EVENT, handleFocusSearch);
  }, [isInitialized]);

  useEffect(() => {
    if (!isInitialized) {
      return;
    }

    void loadPlugins(projectPath);
  }, [isInitialized, loadPlugins, projectPath]);

  useEffect(() => {
    if (!isPluginMenuOpen) {
      return;
    }

    const handleClickOutside = (event: MouseEvent) => {
      if (pluginMenuRef.current && !pluginMenuRef.current.contains(event.target as Node)) {
        setIsPluginMenuOpen(false);
      }
    };

    const handleEsc = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsPluginMenuOpen(false);
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEsc);

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEsc);
    };
  }, [isPluginMenuOpen]);

  const selectedFileInfos = useMemo(() => {
    const fileMap = new Map([...files, ...searchResults].map((file) => [file.path, file]));
    return Array.from(selectedFiles)
      .map((path) => fileMap.get(path))
      .filter((file): file is FileInfo => Boolean(file));
  }, [files, searchResults, selectedFiles]);

  const toolbarActionContext = useMemo(() => ({
    projectPath: projectPath || '',
    currentPath: currentPath || null,
    selectedItems: buildPluginContextItems(selectedFileInfos),
    trigger: 'toolbar',
    pluginScope: '',
    appVersion: APP_VERSION,
  }), [currentPath, projectPath, selectedFileInfos]);

  const visiblePluginActions = useMemo(() => {
    return getVisiblePluginActions(pluginState?.descriptors || [], 'toolbar', toolbarActionContext);
  }, [pluginState?.descriptors, toolbarActionContext]);

  const handleSearch = (value: string) => {
    setLocalSearch(value);
    search(value);
  };

  const handleClearSearch = () => {
    setLocalSearch('');
    clearSearch();
  };

  const handleTogglePluginMenu = async () => {
    if (!projectPath) {
      return;
    }

    if (!isPluginMenuOpen) {
      try {
        await refreshProjectPlugins(projectPath);
      } catch (error) {
        showToast({
          title: '读取插件失败',
          message: String(error),
          tone: 'error',
        });
        return;
      }
    }

    setIsPluginMenuOpen((value) => !value);
  };

  const handleRunPluginAction = (action: typeof visiblePluginActions[number]) => {
    if (!projectPath) {
      return;
    }

    addTask({
      projectPath,
      name: action.title,
      subName: `${action.pluginName} · 工具栏插件`,
      script: {
        kind: 'plugin-action',
        pluginKey: action.pluginKey,
        pluginId: action.pluginId,
        pluginName: action.pluginName,
        commandId: action.commandId,
        commandTitle: action.title,
        location: action.location,
        interactionResponses: [],
        context: {
          ...toolbarActionContext,
          pluginScope: action.scope,
        },
      },
      priority: 'medium',
      maxRetries: 0,
      timeout: 0,
      dependencies: [],
    });

    setIsPluginMenuOpen(false);
    showToast({
      title: '插件任务已加入',
      message: `${action.pluginName} · ${action.title}`,
      tone: 'success',
    });
  };

  const currentPathLabel = currentPath || projectPath || projectName || '';

  return (
    <div className="flex items-center justify-between px-3 py-2 bg-white dark:bg-gray-900">
      <div className="flex-1 min-w-0 pr-4 overflow-hidden">
        {isInitialized && (
          <div className="text-sm text-gray-600 dark:text-gray-400 truncate">
            {currentPathLabel}
          </div>
        )}
      </div>

      <div className="flex items-center gap-2">
        {isInitialized && (
          <div className="flex items-center">
            {isSearchOpen ? (
              <div className="flex items-center gap-1 bg-gray-100 dark:bg-gray-800 rounded-md px-2 py-1">
                <Search className="w-4 h-4 text-gray-400" />
                <input
                  ref={searchInputRef}
                  type="text"
                  value={localSearch}
                  onChange={(e) => handleSearch(e.target.value)}
                  placeholder="搜索文件..."
                  className="bg-transparent border-none outline-none text-sm w-40
                             placeholder:text-gray-400"
                  autoFocus
                />
                {localSearch && (
                  <button onClick={handleClearSearch} className="text-gray-400 hover:text-gray-600">
                    <X className="w-3 h-3" />
                  </button>
                )}
                <button
                  onClick={() => {
                    setIsSearchOpen(false);
                    handleClearSearch();
                  }}
                  className="text-gray-400 hover:text-gray-600"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>
            ) : (
              <button
                onClick={() => setIsSearchOpen(true)}
                className="p-1.5 text-gray-600 hover:text-gray-900 dark:text-gray-400
                           dark:hover:text-gray-100 hover:bg-gray-100 dark:hover:bg-gray-800
                           rounded-md transition-colors"
                title="搜索"
              >
                <Search className="w-4 h-4" />
              </button>
            )}
          </div>
        )}

        {isInitialized && (
          <div className="flex items-center bg-gray-100 dark:bg-gray-800 rounded-md p-0.5">
            <button
              onClick={() => setViewMode('list')}
              className={`
                p-1.5 rounded transition-colors
                ${viewMode === 'list'
                  ? 'bg-white dark:bg-gray-700 shadow-sm'
                  : 'text-gray-600 dark:text-gray-400 hover:text-gray-900'
                }
              `}
              title="列表视图"
            >
              <List className="w-4 h-4" />
            </button>
            <button
              onClick={() => setViewMode('grid')}
              className={`
                p-1.5 rounded transition-colors
                ${viewMode === 'grid'
                  ? 'bg-white dark:bg-gray-700 shadow-sm'
                  : 'text-gray-600 dark:text-gray-400 hover:text-gray-900'
                }
              `}
              title="网格视图"
            >
              <Grid className="w-4 h-4" />
            </button>
          </div>
        )}

        {isInitialized && (
          <div ref={pluginMenuRef} className="relative">
            <button
              onClick={() => void handleTogglePluginMenu()}
              disabled={!projectPath}
              className="flex items-center gap-1.5 p-1.5 text-gray-600 hover:text-gray-900 dark:text-gray-400
                         dark:hover:text-gray-100 hover:bg-gray-100 dark:hover:bg-gray-800
                         rounded-md transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              title={projectPath ? '插件动作' : '请先打开项目'}
            >
              <Puzzle className="w-4 h-4" />
              <span className="hidden sm:inline">插件</span>
              <ChevronDown className="w-3.5 h-3.5" />
            </button>

            {isPluginMenuOpen && projectPath && (
              <div className="absolute right-0 top-full z-30 mt-2 w-80 rounded-xl border border-gray-200 bg-white p-2 shadow-xl dark:border-gray-700 dark:bg-gray-900">
                <div className="mb-2 flex items-center justify-between px-2 py-1">
                  <div>
                    <p className="text-sm font-semibold text-gray-900 dark:text-gray-100">插件动作</p>
                    <p className="text-xs text-gray-500 dark:text-gray-400">
                      {pluginState?.isLoading ? '正在刷新插件列表…' : `可用动作 ${visiblePluginActions.length} 个`}
                    </p>
                  </div>
                </div>

                {visiblePluginActions.length === 0 ? (
                  <div className="rounded-lg bg-gray-50 px-3 py-3 text-sm text-gray-500 dark:bg-gray-800 dark:text-gray-400">
                    当前上下文没有可运行的插件动作。
                  </div>
                ) : (
                  <div className="max-h-80 space-y-1 overflow-auto">
                    {visiblePluginActions.map((action) => (
                      <button
                        key={action.id}
                        onClick={() => handleRunPluginAction(action)}
                        className="w-full rounded-lg px-3 py-2 text-left transition-colors hover:bg-gray-100 dark:hover:bg-gray-800"
                      >
                        <div className="flex items-center justify-between gap-3">
                          <div className="min-w-0">
                            <p className="truncate text-sm font-medium text-gray-900 dark:text-gray-100">
                              {action.title}
                            </p>
                            <p className="truncate text-xs text-gray-500 dark:text-gray-400">
                              {action.pluginName}
                            </p>
                          </div>
                          <span className="rounded-full bg-blue-50 px-2 py-0.5 text-[10px] text-blue-600 dark:bg-blue-900/20 dark:text-blue-300">
                            {action.scope === 'project' ? '项目' : '全局'}
                          </span>
                        </div>
                        {action.description ? (
                          <p className="mt-1 max-h-10 overflow-hidden text-xs text-gray-500 dark:text-gray-400">
                            {action.description}
                          </p>
                        ) : null}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        {isInitialized && (
          <button
            onClick={() => setIsSettingsOpen(true)}
            className="p-1.5 text-gray-600 hover:text-gray-900 dark:text-gray-400
                       dark:hover:text-gray-100 hover:bg-gray-100 dark:hover:bg-gray-800
                       rounded-md transition-colors"
            title="项目设置"
          >
            <Settings className="w-4 h-4" />
          </button>
        )}

      </div>

      <SettingsPanel
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        defaultScope="project"
        onOpenProject={onOpenProject}
      />
    </div>
  );
}
