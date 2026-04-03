import { useState, useEffect } from 'react';
import { X, Plus, Trash2, FolderOpen, FileWarning, HelpCircle } from 'lucide-react';
import { useProjectStoreShallow } from '../../stores/projectStore';
import {
  PRESET_EXCLUDE_PATTERNS,
  getExcludeStorageKey,
  readProjectExcludePatterns,
} from '../../utils/excludePatterns';

// 获取项目的排除规则
function getExcludePatterns(projectPath: string): string[] {
  return readProjectExcludePatterns(projectPath);
}

interface ProjectSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

// 导出供其他组件使用
export { getExcludePatterns };

// 预设的排除规则
export function ProjectSettings({ isOpen, onClose }: ProjectSettingsProps) {
  const { projectPath, refresh } = useProjectStoreShallow((state) => ({
    projectPath: state.projectPath,
    refresh: state.refresh,
  }));
  
  // 排除规则列表
  const [excludePatterns, setExcludePatterns] = useState<string[]>([]);
  const [newPattern, setNewPattern] = useState('');
  const [showPresets, setShowPresets] = useState(false);

  // 加载保存的规则
  useEffect(() => {
    if (isOpen && projectPath) {
      const saved = localStorage.getItem(`project_exclude_${projectPath}`);
      if (saved) {
        setExcludePatterns(JSON.parse(saved));
      } else {
        setExcludePatterns([]);
      }
    }
  }, [isOpen, projectPath]);

  // 应用设置并关闭
  const handleApply = async () => {
    await refresh();
    onClose();
  };

  // 保存规则
  const savePatterns = (patterns: string[]) => {
    setExcludePatterns(patterns);
    if (projectPath) {
      localStorage.setItem(getExcludeStorageKey(projectPath), JSON.stringify(patterns));
    }
  };

  // 添加规则
  const addPattern = (pattern: string) => {
    if (pattern && !excludePatterns.includes(pattern)) {
      savePatterns([...excludePatterns, pattern]);
    }
    setNewPattern('');
    setShowPresets(false);
  };

  // 删除规则
  const removePattern = (pattern: string) => {
    savePatterns(excludePatterns.filter(p => p !== pattern));
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl w-[500px] max-w-[90vw] max-h-[80vh] flex flex-col">
        {/* 头部 */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-2">
            <FolderOpen className="w-5 h-5 text-blue-500" />
            <h3 className="font-semibold text-gray-900 dark:text-gray-100">项目设置</h3>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* 内容 */}
        <div className="p-5 overflow-auto flex-1">
          {/* 排除规则说明 */}
          <div className="flex items-start gap-2 mb-4 p-3 bg-blue-50 dark:bg-blue-900/20 rounded-lg">
            <HelpCircle className="w-5 h-5 text-blue-500 flex-shrink-0 mt-0.5" />
            <div className="text-sm text-blue-700 dark:text-blue-300">
              <p className="font-medium mb-1">排除规则说明</p>
              <p>设置的排除规则将用于：</p>
              <ul className="list-disc list-inside mt-1 space-y-0.5 text-blue-600 dark:text-blue-400">
                <li>文件列表中隐藏匹配的文件/目录</li>
                <li>文件监控忽略这些路径的变更</li>
                <li>搜索时跳过这些文件</li>
              </ul>
            </div>
          </div>

          {/* 添加规则 */}
          <div className="mb-4">
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
              添加排除规则
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={newPattern}
                onChange={(e) => setNewPattern(e.target.value)}
                placeholder="例如: *.log 或 /tmp"
                className="flex-1 px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
                           bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100
                           focus:outline-none focus:ring-2 focus:ring-blue-500"
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    addPattern(newPattern);
                  }
                }}
              />
              <button
                onClick={() => addPattern(newPattern)}
                disabled={!newPattern}
                className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-300 disabled:cursor-not-allowed
                           text-white text-sm font-medium rounded-lg transition-colors"
              >
                <Plus className="w-4 h-4" />
              </button>
            </div>
            
            {/* 预设规则 */}
            <div className="mt-2">
              <button
                onClick={() => setShowPresets(!showPresets)}
                className="text-sm text-blue-600 dark:text-blue-400 hover:underline"
              >
                {showPresets ? '隐藏预设规则' : '从预设选择...'}
              </button>
              
              {showPresets && (
                <div className="mt-2 p-2 bg-gray-50 dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700">
                  <div className="grid grid-cols-1 gap-1">
                    {PRESET_EXCLUDE_PATTERNS.map((preset) => (
                      <button
                        key={preset.value}
                        onClick={() => addPattern(preset.value)}
                        disabled={excludePatterns.includes(preset.value)}
                        className="flex items-center justify-between px-3 py-2 text-left text-sm
                                   hover:bg-gray-100 dark:hover:bg-gray-700 rounded-md
                                   disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        <div>
                          <span className="font-medium text-gray-700 dark:text-gray-200">
                            {preset.label}
                          </span>
                          <span className="ml-2 text-xs text-gray-400">
                            {preset.desc}
                          </span>
                        </div>
                        {excludePatterns.includes(preset.value) ? (
                          <span className="text-xs text-green-600">已添加</span>
                        ) : (
                          <Plus className="w-4 h-4 text-gray-400" />
                        )}
                      </button>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* 当前规则列表 */}
          <div>
            <div className="flex items-center justify-between mb-2">
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
                当前排除规则 ({excludePatterns.length})
              </label>
              {excludePatterns.length > 0 && (
                <button
                  onClick={() => {
                    if (confirm('确定要清空所有排除规则吗？')) {
                      savePatterns([]);
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
                <FileWarning className="w-8 h-8 mb-2 opacity-50" />
                <p className="text-sm">暂无排除规则</p>
              </div>
            ) : (
              <div className="space-y-1 max-h-[200px] overflow-auto">
                {excludePatterns.map((pattern) => (
                  <div
                    key={pattern}
                    className="flex items-center justify-between px-3 py-2 
                               bg-gray-50 dark:bg-gray-800 rounded-lg
                               border border-gray-200 dark:border-gray-700"
                  >
                    <code className="text-sm font-mono text-gray-700 dark:text-gray-300">
                      {pattern}
                    </code>
                    <button
                      onClick={() => removePattern(pattern)}
                      className="p-1 text-gray-400 hover:text-red-600 hover:bg-red-50 
                                 dark:hover:bg-red-900/20 rounded transition-colors"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* 底部按钮 */}
        <div className="flex justify-end gap-2 px-5 py-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50 rounded-b-xl">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm font-medium text-gray-700 dark:text-gray-300
                       hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
          >
            取消
          </button>
          <button
            onClick={handleApply}
            className="px-4 py-2 text-sm font-medium text-white bg-blue-600 
                       hover:bg-blue-700 rounded-lg transition-colors"
          >
            应用
          </button>
        </div>
      </div>
    </div>
  );
}
