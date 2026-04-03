import { useEffect, useState } from 'react';
import { useProjectStoreShallow } from '../../stores/projectStore';
import { SettingsPanel } from '../SettingsPanel';
import {
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
    currentPath,
    projectPath,
    projectName,
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
    search: state.search,
    searchQuery: state.searchQuery,
    clearSearch: state.clearSearch,
    isInitialized: state.isInitialized,
  }));

  const [isSearchOpen, setIsSearchOpen] = useState(false);
  const [localSearch, setLocalSearch] = useState('');
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  useEffect(() => {
    setLocalSearch(searchQuery);
  }, [searchQuery]);

  useEffect(() => {
    setLocalSearch(searchQuery);
    setIsSearchOpen(Boolean(searchQuery));
  }, [projectPath]);

  const handleSearch = (value: string) => {
    setLocalSearch(value);
    search(value);
  };

  const handleClearSearch = () => {
    setLocalSearch('');
    clearSearch();
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
      />
    </div>
  );
}
