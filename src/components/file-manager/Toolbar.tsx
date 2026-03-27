import { useState, useEffect } from 'react';
import { useProjectStore } from '../../stores/projectStore';
import { SettingsPanel } from '../SettingsPanel';
import { getParentPath, getPathLabel } from './dragDrop';
import {
  ArrowLeft,
  ArrowUp,
  RefreshCw,
  List,
  Grid,
  Search,
  X,
  Settings,
} from 'lucide-react';

export function Toolbar() {
  const {
    viewMode,
    setViewMode,
    refresh,
    currentPath,
    projectPath,
    projectName,
    loadDirectory,
    search,
    searchQuery,
    clearSearch,
    closeProject,
    isInitialized,
  } = useProjectStore();

  const [isSearchOpen, setIsSearchOpen] = useState(false);
  const [localSearch, setLocalSearch] = useState('');
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  const handleSearch = (value: string) => {
    setLocalSearch(value);
    search(value);
  };

  const handleClearSearch = () => {
    setLocalSearch('');
    clearSearch();
  };

  const atProjectRoot = !currentPath || !projectPath || currentPath === projectPath;
  const currentPathLabel = getPathLabel(currentPath, projectPath, projectName);

  const handleGoUp = () => {
    if (atProjectRoot || !currentPath) {
      return;
    }

    void loadDirectory(getParentPath(currentPath));
  };

  return (
    <div className="flex items-center justify-between px-3 py-2 border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900">
      {/* 左侧 - 返回项目列表 */}
      <div className="flex items-center gap-2">
        {isInitialized && (
          <button
            onClick={closeProject}
            className="p-1.5 text-gray-700 hover:text-gray-900 hover:bg-gray-100 
                       dark:text-gray-300 dark:hover:text-gray-100 dark:hover:bg-gray-800
                       rounded-md transition-colors"
            title="返回项目列表"
          >
            <ArrowLeft className="w-4 h-4" />
          </button>
        )}

        {isInitialized && (
          <button
            onClick={handleGoUp}
            disabled={atProjectRoot}
            className="p-1.5 text-gray-600 hover:text-gray-900 dark:text-gray-400
                       dark:hover:text-gray-100 hover:bg-gray-100 dark:hover:bg-gray-800
                       rounded-md transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            title={atProjectRoot ? '已经在项目根目录' : '返回上级目录'}
          >
            <ArrowUp className="w-4 h-4" />
          </button>
        )}

        {isInitialized && (
          <button
            onClick={refresh}
            className="p-1.5 text-gray-600 hover:text-gray-900 dark:text-gray-400
                       dark:hover:text-gray-100 hover:bg-gray-100 dark:hover:bg-gray-800
                       rounded-md transition-colors"
            title="刷新"
          >
            <RefreshCw className="w-4 h-4" />
          </button>
        )}
      </div>

      {/* 中间 - 面包屑/路径 */}
      <div className="flex-1 px-4 overflow-hidden">
        {isInitialized && (
          <div className="text-sm text-gray-600 dark:text-gray-400 truncate">
            {currentPathLabel}
          </div>
        )}
      </div>

      {/* 右侧 */}
      <div className="flex items-center gap-2">
        {/* 搜索 */}
        {isInitialized && (
          <div className="flex items-center">
            {isSearchOpen ? (
              <div className="flex items-center gap-1 bg-gray-100 dark:bg-gray-800 rounded-md px-2 py-1">
                <Search className="w-4 h-4 text-gray-400" />
                <input
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

        {/* 视图切换 */}
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

        {/* 设置 */}
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

        {/* 快捷启动器和任务按钮将由 FileManager 添加 */}
      </div>

      {/* 统一设置面板 */}
      <SettingsPanel
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        defaultScope="project"
      />
    </div>
  );
}
