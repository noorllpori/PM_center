import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useProjectStore } from '../stores/projectStore';
import { FileChange } from '../types';
import { Clock, FilePlus, FileEdit, FileMinus, RefreshCw, Filter, Archive } from 'lucide-react';

// 格式化时间
function formatTime(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  
  // 小于1分钟
  if (diff < 60 * 1000) {
    return '刚刚';
  }
  
  // 小于1小时
  if (diff < 60 * 60 * 1000) {
    return `${Math.floor(diff / (60 * 1000))}分钟前`;
  }
  
  // 小于24小时
  if (diff < 24 * 60 * 60 * 1000) {
    return `${Math.floor(diff / (60 * 60 * 1000))}小时前`;
  }
  
  // 小于7天
  if (diff < 7 * 24 * 60 * 60 * 1000) {
    return `${Math.floor(diff / (24 * 60 * 60 * 1000))}天前`;
  }
  
  // 显示日期
  return date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

// 格式化文件大小
function formatSize(bytes: number | null): string {
  if (!bytes) return '-';
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)}MB`;
}

// 获取变更类型图标和颜色
function getChangeTypeInfo(type: string) {
  switch (type) {
    case 'created':
      return { icon: FilePlus, color: 'text-green-600', bgColor: 'bg-green-50', label: '新增' };
    case 'modified':
      return { icon: FileEdit, color: 'text-blue-600', bgColor: 'bg-blue-50', label: '修改' };
    case 'deleted':
      return { icon: FileMinus, color: 'text-red-600', bgColor: 'bg-red-50', label: '删除' };
    default:
      return { icon: FileEdit, color: 'text-gray-600', bgColor: 'bg-gray-50', label: type };
  }
}

export function ChangeLog() {
  const { projectPath, projectName } = useProjectStore();
  const [changes, setChanges] = useState<FileChange[]>([]);
  const [stats, setStats] = useState({ total: 0, created: 0, modified: 0, deleted: 0 });
  const [filter, setFilter] = useState<'all' | 'created' | 'modified' | 'deleted'>('all');
  const [isLoading, setIsLoading] = useState(false);
  const [since, setSince] = useState(24); // 默认显示最近24小时

  // 加载变更日志
  const loadChanges = async () => {
    if (!projectPath) {
      console.log('No project path');
      return;
    }
    
    console.log('Loading changes for:', projectPath);
    setIsLoading(true);
    try {
      const sinceTimestamp = Math.floor(Date.now() / 1000) - since * 3600;
      console.log('Since timestamp:', sinceTimestamp);
      
      const result = await invoke<FileChange[]>('get_file_changes', {
        projectPath,
        since: sinceTimestamp,
        changeType: filter === 'all' ? null : filter,
        limit: 500,
      });
      
      console.log('Changes loaded:', result.length);
      setChanges(result);
      
      // 加载统计
      const statsResult = await invoke<{ total: number; created: number; modified: number; deleted: number }>('get_change_stats', {
        projectPath,
        since: sinceTimestamp,
      });
      
      console.log('Stats:', statsResult);
      setStats(statsResult);
    } catch (error) {
      console.error('Failed to load changes:', error);
    } finally {
      setIsLoading(false);
    }
  };

  // 手动归档
  const handleArchive = async () => {
    try {
      const count = await invoke<number>('archive_old_changes');
      alert(`已归档 ${count} 条旧记录`);
      loadChanges();
    } catch (error) {
      console.error('Failed to archive:', error);
    }
  };

  // 加载数据
  useEffect(() => {
    loadChanges();
    // 注意：这里不设置自动刷新间隔，用户需要手动点击刷新
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectPath, filter, since]);

  // 按时间分组
  const groupedChanges = changes.reduce((groups, change) => {
    const date = new Date(change.timestamp * 1000).toLocaleDateString('zh-CN');
    if (!groups[date]) {
      groups[date] = [];
    }
    groups[date].push(change);
    return groups;
  }, {} as Record<string, FileChange[]>);

  return (
    <div className="flex h-full w-full min-w-0 flex-col bg-white dark:bg-gray-900">
      {/* 统计卡片 */}
      <div className="grid grid-cols-4 gap-4 p-4 border-b border-gray-200 dark:border-gray-700">
        <div className="bg-gray-50 dark:bg-gray-800 rounded-lg p-3">
          <div className="text-2xl font-bold text-gray-900 dark:text-gray-100">{stats.total}</div>
          <div className="text-xs text-gray-500">总变更</div>
        </div>
        <div className="bg-green-50 dark:bg-green-900/20 rounded-lg p-3">
          <div className="text-2xl font-bold text-green-600">{stats.created}</div>
          <div className="text-xs text-green-600/70">新增</div>
        </div>
        <div className="bg-blue-50 dark:bg-blue-900/20 rounded-lg p-3">
          <div className="text-2xl font-bold text-blue-600">{stats.modified}</div>
          <div className="text-xs text-blue-600/70">修改</div>
        </div>
        <div className="bg-red-50 dark:bg-red-900/20 rounded-lg p-3">
          <div className="text-2xl font-bold text-red-600">{stats.deleted}</div>
          <div className="text-xs text-red-600/70">删除</div>
        </div>
      </div>

      {/* 工具栏 */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700">
        <div className="flex items-center gap-4">
          {/* 时间范围选择 */}
          <div className="flex items-center gap-2">
            <Clock className="w-4 h-4 text-gray-400" />
            <select
              value={since}
              onChange={(e) => setSince(Number(e.target.value))}
              className="text-sm border border-gray-300 dark:border-gray-600 rounded px-2 py-1"
            >
              <option value={1}>最近1小时</option>
              <option value={24}>最近24小时</option>
              <option value={72}>最近3天</option>
              <option value={168}>最近7天</option>
              <option value={360}>最近15天</option>
            </select>
          </div>

          {/* 类型过滤 */}
          <div className="flex items-center gap-2">
            <Filter className="w-4 h-4 text-gray-400" />
            <div className="flex bg-gray-100 dark:bg-gray-800 rounded p-0.5">
              {(['all', 'created', 'modified', 'deleted'] as const).map((type) => (
                <button
                  key={type}
                  onClick={() => setFilter(type)}
                  className={`
                    px-3 py-1 text-xs rounded transition-colors
                    ${filter === type
                      ? 'bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 shadow-sm'
                      : 'text-gray-600 dark:text-gray-400 hover:text-gray-900'
                    }
                  `}
                >
                  {type === 'all' ? '全部' : type === 'created' ? '新增' : type === 'modified' ? '修改' : '删除'}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={handleArchive}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs
                       text-gray-600 hover:text-gray-900 hover:bg-gray-100
                       dark:text-gray-400 dark:hover:text-gray-100 dark:hover:bg-gray-800
                       rounded transition-colors"
            title="归档15天前的记录"
          >
            <Archive className="w-4 h-4" />
            归档旧记录
          </button>
          <button
            onClick={loadChanges}
            disabled={isLoading}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs
                       text-gray-600 hover:text-gray-900 hover:bg-gray-100
                       dark:text-gray-400 dark:hover:text-gray-100 dark:hover:bg-gray-800
                       rounded transition-colors"
          >
            <RefreshCw className={`w-4 h-4 ${isLoading ? 'animate-spin' : ''}`} />
            刷新
          </button>
        </div>
      </div>

      {/* 日志列表 */}
      <div className="flex-1 overflow-auto p-4">
        {Object.entries(groupedChanges).length === 0 ? (
          <div className="flex items-center justify-center h-full text-gray-400">
            <div className="text-center">
              <Clock className="w-12 h-12 mx-auto mb-3 opacity-50" />
              <p>暂无变更记录</p>
              <p className="text-xs mt-1">文件监控正在后台运行...</p>
            </div>
          </div>
        ) : (
          <div className="space-y-6">
            {Object.entries(groupedChanges).map(([date, dayChanges]) => (
              <div key={date}>
                <div className="sticky top-0 bg-white dark:bg-gray-900 py-2 border-b border-gray-200 dark:border-gray-700 mb-3">
                  <span className="text-sm font-medium text-gray-500">{date}</span>
                  <span className="text-xs text-gray-400 ml-2">({dayChanges.length} 条)</span>
                </div>
                <div className="space-y-2">
                  {dayChanges.map((change) => {
                    const { icon: Icon, color, bgColor, label } = getChangeTypeInfo(change.change_type);
                    const fileName = change.file_path.split(/[\\/]/).pop() || change.file_path;
                    const dirPath = change.file_path.substring(0, change.file_path.length - fileName.length);
                    
                    return (
                      <div
                        key={change.id}
                        className="flex items-center gap-3 p-3 rounded-lg
                                   hover:bg-gray-50 dark:hover:bg-gray-800
                                   border border-transparent hover:border-gray-200 dark:hover:border-gray-700
                                   transition-colors"
                      >
                        <div className={`w-8 h-8 rounded-lg ${bgColor} flex items-center justify-center flex-shrink-0`}>
                          <Icon className={`w-4 h-4 ${color}`} />
                        </div>
                        
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2">
                            <span className="font-medium text-sm text-gray-900 dark:text-gray-100 truncate">
                              {fileName}
                            </span>
                            <span className={`text-xs px-1.5 py-0.5 rounded ${bgColor} ${color}`}>
                              {label}
                            </span>
                          </div>
                          <div className="text-xs text-gray-500 truncate mt-0.5">
                            {dirPath}
                          </div>
                        </div>
                        
                        <div className="text-right flex-shrink-0">
                          <div className="text-xs text-gray-500">
                            {formatTime(change.timestamp)}
                          </div>
                          {change.file_size && (
                            <div className="text-xs text-gray-400 mt-0.5">
                              {formatSize(change.file_size)}
                            </div>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
