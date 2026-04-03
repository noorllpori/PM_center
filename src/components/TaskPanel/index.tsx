import { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import { useTaskStore, initTaskEventListeners } from '../../stores/taskStore';
import { useOptionalProjectStoreShallow } from '../../stores/projectStore';
import type { Task, TaskStatus, TaskPriority, ProjectScript } from '../../types/task';
import { getProjectScripts } from '../../api/scripts';
import { invoke } from '@tauri-apps/api/core';
import { 
  Square, 
  RotateCcw, 
  Trash2, 
  Plus, 
  X, 
  Terminal,
  Clock,
  AlertCircle,
  CheckCircle,
  Loader2,
  ChevronRight,
  ChevronDown,
  FileCode,
  Eraser,
  Folder,
  FileText,
  Code,
  RefreshCw,
} from 'lucide-react';

interface TaskPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

const ALL_PROJECTS_VALUE = '__all_projects__';

function getProjectDisplayName(
  targetProjectPath: string,
  currentProjectPath: string | null,
  currentProjectName: string | null,
) {
  if (currentProjectPath && targetProjectPath === currentProjectPath && currentProjectName) {
    return currentProjectName;
  }

  const parts = targetProjectPath.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || targetProjectPath;
}

export function TaskPanel({ isOpen, onClose }: TaskPanelProps) {
  const { projectPath, projectName } = useOptionalProjectStoreShallow((state) => ({
    projectPath: state.projectPath,
    projectName: state.projectName,
  }));
  const { 
    tasks: allTasks, 
    activeTaskId, 
    selectTask, 
    cancelTask, 
    retryTask, 
    removeTask,
    clearCompleted,
  } = useTaskStore();
  const [projectFilter, setProjectFilter] = useState<string>(ALL_PROJECTS_VALUE);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    setProjectFilter(projectPath || ALL_PROJECTS_VALUE);
  }, [isOpen, projectPath]);

  const projectOptions = useMemo(() => {
    const options = new Map<string, string>();

    allTasks.forEach((task) => {
      if (!options.has(task.projectPath)) {
        options.set(
          task.projectPath,
          getProjectDisplayName(task.projectPath, projectPath, projectName),
        );
      }
    });

    if (projectPath) {
      options.set(projectPath, projectName || getProjectDisplayName(projectPath, projectPath, projectName));
    }

    return Array.from(options.entries())
      .map(([path, label]) => ({ path, label }))
      .sort((a, b) => {
        if (projectPath) {
          if (a.path === projectPath && b.path !== projectPath) return -1;
          if (b.path === projectPath && a.path !== projectPath) return 1;
        }

        return a.label.localeCompare(b.label, 'zh-CN');
      });
  }, [allTasks, projectName, projectPath]);

  const selectedProjectPath = projectFilter === ALL_PROJECTS_VALUE ? null : projectFilter;
  const selectedProjectLabel = selectedProjectPath
    ? (projectOptions.find((option) => option.path === selectedProjectPath)?.label
      || getProjectDisplayName(selectedProjectPath, projectPath, projectName))
    : '所有项目';
  const isViewingAllProjects = selectedProjectPath === null;

  // 按当前项目范围显示任务
  const tasks = useMemo(() => {
    if (!selectedProjectPath) {
      return allTasks;
    }

    return allTasks.filter((task) => task.projectPath === selectedProjectPath);
  }, [allTasks, selectedProjectPath]);

  // 计算当前范围统计
  const stats = useMemo(() => ({
    total: tasks.length,
    pending: tasks.filter(t => t.status === 'pending').length,
    running: tasks.filter(t => t.status === 'running').length,
    completed: tasks.filter(t => t.status === 'completed').length,
    failed: tasks.filter(t => t.status === 'failed').length,
    cancelled: tasks.filter(t => t.status === 'cancelled').length,
  }), [tasks]);
  
  const [showNewTask, setShowNewTask] = useState(false);
  const [filter, setFilter] = useState<TaskStatus | 'all'>('all');
  const [showLog, setShowLog] = useState(true);

  useEffect(() => {
    if (projectFilter === ALL_PROJECTS_VALUE) {
      return;
    }

    const filterStillExists = projectOptions.some((option) => option.path === projectFilter);
    if (!filterStillExists) {
      setProjectFilter(projectPath || ALL_PROJECTS_VALUE);
    }
  }, [projectFilter, projectOptions, projectPath]);

  // 初始化事件监听
  useEffect(() => {
    if (isOpen) {
      initTaskEventListeners();
    }
  }, [isOpen]);

  // ESC 关闭
  useEffect(() => {
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleEsc);
    return () => window.removeEventListener('keydown', handleEsc);
  }, [isOpen, onClose]);

  const filteredTasks = filter === 'all' 
    ? tasks 
    : tasks.filter(t => t.status === filter);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    if (!activeTaskId) {
      return;
    }

    const activeTaskVisible = filteredTasks.some((task) => task.id === activeTaskId);
    if (!activeTaskVisible) {
      selectTask(filteredTasks[0]?.id ?? null);
    }
  }, [activeTaskId, filteredTasks, isOpen, selectTask]);

  const activeTask = filteredTasks.find(t => t.id === activeTaskId) ?? null;

  const handleClearCompleted = useCallback(() => {
    if (isViewingAllProjects) {
      clearCompleted();
      return;
    }

    tasks
      .filter((task) => task.status === 'completed')
      .forEach((task) => removeTask(task.id));
  }, [clearCompleted, isViewingAllProjects, removeTask, tasks]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white dark:bg-gray-900 rounded-xl shadow-2xl w-[1200px] max-w-[95vw] h-[800px] max-h-[90vh] flex flex-col">
        {/* 头部 */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-3">
            <Terminal className="w-5 h-5 text-blue-500" />
            <div>
              <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">任务中心</h2>
              <div className="flex items-center gap-1 text-xs text-gray-500 dark:text-gray-400">
                <Folder className="w-3 h-3" />
                当前视图: {selectedProjectLabel}
              </div>
              {projectName && (
                <div className="flex items-center gap-1 text-xs text-gray-500 dark:text-gray-400">
                  <Folder className="w-3 h-3" />
                  当前打开项目: {projectName}
                </div>
              )}
            </div>
            <TaskStats stats={stats} />
          </div>
          
          <div className="flex items-center gap-2">
            {/* 新建任务 */}
            <button
              onClick={() => setShowNewTask(true)}
              disabled={!projectPath}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-300 disabled:cursor-not-allowed text-white text-sm rounded-lg transition-colors"
              title={projectPath ? '在当前打开项目中新建任务' : '请先打开项目'}
            >
              <Plus className="w-4 h-4" />
              新建任务
            </button>
            
            {/* 清理已完成 */}
            {stats.completed > 0 && (
              <button
                onClick={handleClearCompleted}
                className="p-1.5 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg"
                title={isViewingAllProjects ? '清理所有项目中已完成的任务' : '清理当前项目范围中已完成的任务'}
              >
                <Eraser className="w-4 h-4" />
              </button>
            )}
            
            <button
              onClick={onClose}
              className="p-1.5 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* 主体 */}
        <div className="flex-1 flex overflow-hidden">
          {/* 左侧任务列表 */}
          <div className="w-[400px] border-r border-gray-200 dark:border-gray-700 flex flex-col">
            <div className="p-3 border-b border-gray-200 dark:border-gray-700 space-y-2">
              <div className="text-xs font-medium text-gray-500 dark:text-gray-400">项目范围</div>
              <select
                value={projectFilter}
                onChange={(event) => setProjectFilter(event.target.value)}
                className="w-full rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-sm text-gray-900 dark:text-gray-100"
              >
                <option value={ALL_PROJECTS_VALUE}>所有项目</option>
                {projectOptions.map((option) => (
                  <option key={option.path} value={option.path}>
                    {option.label}
                  </option>
                ))}
              </select>
              {projectPath && selectedProjectPath && selectedProjectPath !== projectPath && (
                <p className="text-xs text-amber-600 dark:text-amber-400">
                  当前打开项目仍是 {projectName || selectedProjectLabel}，这里只是切换查看别的项目任务。
                </p>
              )}
              {!projectPath && (
                <p className="text-xs text-gray-500 dark:text-gray-400">
                  主页模式默认汇总全部项目任务，你也可以按项目单独查看。
                </p>
              )}
            </div>

            {/* 过滤器 */}
            <div className="flex items-center gap-1 p-2 border-b border-gray-200 dark:border-gray-700">
              <FilterButton active={filter === 'all'} onClick={() => setFilter('all')} count={stats.total}>
                全部
              </FilterButton>
              <FilterButton active={filter === 'running'} onClick={() => setFilter('running')} count={stats.running}>
                运行中
              </FilterButton>
              <FilterButton active={filter === 'pending'} onClick={() => setFilter('pending')} count={stats.pending}>
                等待中
              </FilterButton>
              <FilterButton active={filter === 'completed'} onClick={() => setFilter('completed')} count={stats.completed}>
                已完成
              </FilterButton>
              <FilterButton active={filter === 'failed'} onClick={() => setFilter('failed')} count={stats.failed}>
                失败
              </FilterButton>
            </div>

            {/* 任务列表 */}
            <div className="flex-1 overflow-auto">
              {filteredTasks.length === 0 ? (
                <div className="flex flex-col items-center justify-center h-full text-gray-400">
                  <Terminal className="w-12 h-12 mb-3 opacity-50" />
                  <p>暂无任务</p>
                </div>
              ) : (
                <div className="divide-y divide-gray-100 dark:divide-gray-800">
                  {filteredTasks.map(task => (
                    <TaskItem
                      key={task.id}
                      task={task}
                      projectLabel={getProjectDisplayName(task.projectPath, projectPath, projectName)}
                      isActive={task.id === activeTaskId}
                      onClick={() => selectTask(task.id)}
                      onCancel={() => cancelTask(task.id)}
                      onRetry={() => retryTask(task.id)}
                      onRemove={() => removeTask(task.id)}
                    />
                  ))}
                </div>
              )}
            </div>
          </div>

          {/* 右侧详情 */}
          <div className="flex-1 flex flex-col bg-gray-50 dark:bg-gray-950">
            {activeTask ? (
              <>
                {/* 任务信息 */}
                <div className="p-4 bg-white dark:bg-gray-900 border-b border-gray-200 dark:border-gray-700">
                  <div className="flex items-start justify-between mb-3">
                    <div>
                      <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">{activeTask.name}</h3>
                      <p className="text-sm text-gray-500 dark:text-gray-400">{activeTask.subName}</p>
                      <div className="mt-1 flex items-center gap-1 text-xs text-gray-500 dark:text-gray-400">
                        <Folder className="w-3.5 h-3.5" />
                        {getProjectDisplayName(activeTask.projectPath, projectPath, projectName)}
                      </div>
                    </div>
                    <TaskStatusBadge status={activeTask.status} />
                  </div>
                  
                  {/* 进度条 */}
                  <div className="mb-3">
                    <div className="flex items-center justify-between text-sm mb-1">
                      <span className="text-gray-500 dark:text-gray-400">进度</span>
                      <span className="font-medium text-gray-900 dark:text-gray-100">{activeTask.progress}%</span>
                    </div>
                    <div className="h-2 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
                      <div 
                        className={`h-full transition-all duration-300 ${
                          activeTask.status === 'failed' ? 'bg-red-500' :
                          activeTask.status === 'completed' ? 'bg-green-500' :
                          'bg-blue-500'
                        }`}
                        style={{ width: `${activeTask.progress}%` }}
                      />
                    </div>
                  </div>

                  {/* 操作按钮 */}
                  <div className="flex items-center gap-2">
                    {activeTask.status === 'running' && (
                      <button
                        onClick={() => cancelTask(activeTask.id)}
                        className="flex items-center gap-1.5 px-3 py-1.5 text-sm text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors"
                      >
                        <Square className="w-4 h-4" />
                        停止
                      </button>
                    )}
                    {(activeTask.status === 'failed' || activeTask.status === 'cancelled') && (
                      <button
                        onClick={() => retryTask(activeTask.id)}
                        className="flex items-center gap-1.5 px-3 py-1.5 text-sm text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded-lg transition-colors"
                      >
                        <RotateCcw className="w-4 h-4" />
                        重试
                      </button>
                    )}
                    <button
                      onClick={() => removeTask(activeTask.id)}
                      className="flex items-center gap-1.5 px-3 py-1.5 text-sm text-gray-600 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-800 rounded-lg transition-colors"
                    >
                      <Trash2 className="w-4 h-4" />
                      删除
                    </button>
                  </div>
                </div>

                {/* 日志输出 */}
                <div className="flex-1 flex flex-col min-h-0">
                  <div className="flex items-center justify-between px-4 py-2 bg-gray-100 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700">
                    <span className="text-sm font-medium text-gray-700 dark:text-gray-300">输出日志</span>
                    <button
                      onClick={() => setShowLog(!showLog)}
                      className="text-gray-500 hover:text-gray-700"
                    >
                      {showLog ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
                    </button>
                  </div>
                  {showLog && (
                    <LogViewer output={activeTask.output} />
                  )}
                </div>

                {/* 任务信息栏 */}
                <div className="px-4 py-2 bg-gray-100 dark:bg-gray-800 border-t border-gray-200 dark:border-gray-700 text-xs text-gray-500 dark:text-gray-400">
                  <div className="flex items-center gap-4">
                    <span>项目: {getProjectDisplayName(activeTask.projectPath, projectPath, projectName)}</span>
                    <span>ID: {activeTask.id.slice(0, 16)}...</span>
                    <span>类型: {activeTask.script.type}</span>
                    <span>优先级: {activeTask.priority}</span>
                    {activeTask.startedAt && (
                      <span>开始: {new Date(activeTask.startedAt).toLocaleTimeString()}</span>
                    )}
                    {activeTask.exitCode !== undefined && (
                      <span>退出码: {activeTask.exitCode}</span>
                    )}
                  </div>
                </div>
              </>
            ) : (
              <div className="flex-1 flex flex-col items-center justify-center text-gray-400">
                <FileCode className="w-16 h-16 mb-4 opacity-30" />
                <p>选择一个任务查看详情</p>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* 新建任务对话框 */}
      {showNewTask && projectPath && (
        <NewTaskDialog 
          projectPath={projectPath}
          projectName={projectName || ''}
          onClose={() => setShowNewTask(false)} 
        />
      )}
    </div>
  );
}

// 任务统计组件
function TaskStats({ stats }: { stats: { total: number; pending: number; running: number; completed: number; failed: number; cancelled: number } }) {
  return (
    <div className="flex items-center gap-2 text-xs">
      {stats.running > 0 && (
        <span className="px-2 py-0.5 bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 rounded-full">
          运行中 {stats.running}
        </span>
      )}
      {stats.pending > 0 && (
        <span className="px-2 py-0.5 bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400 rounded-full">
          等待 {stats.pending}
        </span>
      )}
      {stats.failed > 0 && (
        <span className="px-2 py-0.5 bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400 rounded-full">
          失败 {stats.failed}
        </span>
      )}
    </div>
  );
}

// 过滤器按钮
function FilterButton({ 
  active, 
  onClick, 
  children, 
  count 
}: { 
  active: boolean; 
  onClick: () => void; 
  children: React.ReactNode;
  count: number;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex-1 py-1.5 px-2 text-sm rounded-lg transition-colors ${
        active 
          ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400' 
          : 'text-gray-600 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-800'
      }`}
    >
      {children}
      {count > 0 && <span className="ml-1 text-xs opacity-70">({count})</span>}
    </button>
  );
}

// 任务项
function TaskItem({ 
  task, 
  projectLabel,
  isActive, 
  onClick, 
  onCancel, 
  onRetry, 
  onRemove 
}: { 
  task: Task;
  projectLabel: string;
  isActive: boolean;
  onClick: () => void;
  onCancel: () => void;
  onRetry: () => void;
  onRemove: () => void;
}) {
  return (
    <div 
      onClick={onClick}
      className={`p-3 cursor-pointer transition-colors ${
        isActive 
          ? 'bg-blue-50 dark:bg-blue-900/20 border-l-4 border-blue-500' 
          : 'hover:bg-gray-50 dark:hover:bg-gray-800 border-l-4 border-transparent'
      }`}
    >
      <div className="flex items-start justify-between mb-1">
        <div className="flex items-center gap-2">
          <TaskStatusIcon status={task.status} />
          <span className="font-medium text-gray-900 dark:text-gray-100 truncate max-w-[200px]">{task.name}</span>
        </div>
        <span className="text-xs text-gray-500 dark:text-gray-400">{task.progress}%</span>
      </div>
      
      <p className="text-xs text-gray-500 dark:text-gray-400 truncate mb-2">{task.subName}</p>
      <div className="mb-2 flex items-center gap-1 text-[11px] text-gray-500 dark:text-gray-400">
        <Folder className="w-3 h-3" />
        <span className="truncate">{projectLabel}</span>
      </div>
      
      {/* 进度条 */}
      <div className="h-1 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden mb-2">
        <div 
          className={`h-full transition-all ${
            task.status === 'failed' ? 'bg-red-500' :
            task.status === 'completed' ? 'bg-green-500' :
            'bg-blue-500'
          }`}
          style={{ width: `${task.progress}%` }}
        />
      </div>

      {/* 操作按钮 */}
      <div className="flex items-center gap-1">
        {task.status === 'running' && (
          <button
            onClick={(e) => { e.stopPropagation(); onCancel(); }}
            className="p-1 text-gray-400 hover:text-red-500"
            title="停止"
          >
            <Square className="w-3.5 h-3.5" />
          </button>
        )}
        {(task.status === 'failed' || task.status === 'cancelled') && (
          <button
            onClick={(e) => { e.stopPropagation(); onRetry(); }}
            className="p-1 text-gray-400 hover:text-blue-500"
            title="重试"
          >
            <RotateCcw className="w-3.5 h-3.5" />
          </button>
        )}
        <button
          onClick={(e) => { e.stopPropagation(); onRemove(); }}
          className="p-1 text-gray-400 hover:text-red-500"
          title="删除"
        >
          <Trash2 className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
}

// 状态图标
function TaskStatusIcon({ status }: { status: TaskStatus }) {
  switch (status) {
    case 'running':
      return <Loader2 className="w-4 h-4 text-blue-500 animate-spin" />;
    case 'completed':
      return <CheckCircle className="w-4 h-4 text-green-500" />;
    case 'failed':
      return <AlertCircle className="w-4 h-4 text-red-500" />;
    case 'cancelled':
      return <Square className="w-4 h-4 text-gray-500" />;
    case 'pending':
      return <Clock className="w-4 h-4 text-yellow-500" />;
    default:
      return null;
  }
}

// 状态标签
function TaskStatusBadge({ status }: { status: TaskStatus }) {
  const styles = {
    pending: 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400',
    running: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400',
    completed: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400',
    failed: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400',
    cancelled: 'bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-400',
  };

  const labels = {
    pending: '等待中',
    running: '运行中',
    completed: '已完成',
    failed: '失败',
    cancelled: '已取消',
  };

  return (
    <span className={`px-2.5 py-1 text-xs font-medium rounded-full ${styles[status]}`}>
      {labels[status]}
    </span>
  );
}

// 日志查看器
function LogViewer({ output }: { output: string[] }) {
  const scrollRef = useRef<HTMLDivElement>(null);

  // 自动滚动到底部
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [output]);

  return (
    <div 
      ref={scrollRef}
      className="flex-1 overflow-auto p-4 font-mono text-sm bg-gray-950 text-gray-300"
    >
      {output.length === 0 ? (
        <span className="text-gray-600">等待输出...</span>
      ) : (
        output.map((line, i) => (
          <div key={i} className="whitespace-pre-wrap break-all">
            <span className="text-gray-600 select-none mr-2">{(i + 1).toString().padStart(4, '0')}</span>
            {line}
          </div>
        ))
      )}
      <div className="animate-pulse">▋</div>
    </div>
  );
}

// 新建任务对话框
function NewTaskDialog({ 
  projectPath, 
  projectName,
  onClose 
}: { 
  projectPath: string;
  projectName: string;
  onClose: () => void;
}) {
  const addTask = useTaskStore(state => state.addTask);
  
  // 模式切换：选择脚本 vs 自定义代码
  const [mode, setMode] = useState<'select' | 'custom'>('select');
  
  // 脚本列表
  const [scripts, setScripts] = useState<ProjectScript[]>([]);
  const [loadingScripts, setLoadingScripts] = useState(false);
  const [selectedScriptId, setSelectedScriptId] = useState<string | null>(null);
  
  // 表单字段
  const [name, setName] = useState('');
  const [subName, setSubName] = useState('');
  const [code, setCode] = useState('');
  const [priority, setPriority] = useState<TaskPriority>('medium');
  const [timeout, setTimeout] = useState(0);

  // 加载脚本列表
  const loadScripts = async () => {
    setLoadingScripts(true);
    try {
      const list = await getProjectScripts(projectPath);
      setScripts(list);
    } catch (err) {
      console.error('加载脚本失败:', err);
    } finally {
      setLoadingScripts(false);
    }
  };

  useEffect(() => {
    loadScripts();
  }, [projectPath]);

  // 选择脚本时更新表单
  const handleSelectScript = (script: ProjectScript) => {
    setSelectedScriptId(script.id);
    setName(script.name);
    // 读取脚本内容
    invoke<string>('read_file', { path: script.path })
      .then(content => setCode(content))
      .catch(err => console.error('读取脚本失败:', err));
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name || !code) return;

    addTask({
      projectPath,
      name,
      subName,
      script: {
        code,
        type: 'python',
      },
      priority,
      maxRetries: 0,
      timeout: timeout * 60, // 分钟转秒
      dependencies: [],
    });

    onClose();
  };

  // Python 示例代码
  const exampleCode = `# Python 任务示例
import time
import sys

print("Starting task...", flush=True)

for i in range(0, 101, 10):
    print(f"progress /***{i}*/", flush=True)
    time.sleep(0.2)

print("Task completed!", flush=True)
sys.exit(0)`;

  const loadExample = () => {
    setCode(exampleCode);
  };

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50">
      <div className="bg-white dark:bg-gray-900 rounded-xl shadow-xl w-[800px] max-w-[95vw] max-h-[90vh] flex flex-col">
        <div className="flex items-center justify-between px-5 py-3 border-b border-gray-200 dark:border-gray-700">
          <h3 className="text-lg font-semibold text-gray-900 dark:text-gray-100">新建任务</h3>
          <button onClick={onClose} className="text-gray-500 hover:text-gray-700">
            <X className="w-5 h-5" />
          </button>
        </div>

        <div className="flex-1 overflow-hidden flex">
          {/* 左侧：模式选择和脚本列表 */}
          <div className="w-[320px] border-r border-gray-200 dark:border-gray-700 flex flex-col">
            {/* 模式切换 */}
            <div className="p-3 border-b border-gray-200 dark:border-gray-700">
              <div className="flex bg-gray-100 dark:bg-gray-800 rounded-lg p-1">
                <button
                  onClick={() => setMode('select')}
                  className={`flex-1 flex items-center justify-center gap-2 py-1.5 text-sm rounded-md transition-colors ${
                    mode === 'select'
                      ? 'bg-white dark:bg-gray-700 text-blue-600 shadow-sm'
                      : 'text-gray-600 dark:text-gray-400 hover:text-gray-900'
                  }`}
                >
                  <FileText className="w-4 h-4" />
                  选择脚本
                </button>
                <button
                  onClick={() => setMode('custom')}
                  className={`flex-1 flex items-center justify-center gap-2 py-1.5 text-sm rounded-md transition-colors ${
                    mode === 'custom'
                      ? 'bg-white dark:bg-gray-700 text-blue-600 shadow-sm'
                      : 'text-gray-600 dark:text-gray-400 hover:text-gray-900'
                  }`}
                >
                  <Code className="w-4 h-4" />
                  自定义
                </button>
              </div>
            </div>

            {mode === 'select' ? (
              <>
                {/* 脚本列表头部 */}
                <div className="px-3 py-2 border-b border-gray-200 dark:border-gray-700 flex items-center justify-between">
                  <span className="text-xs font-medium text-gray-500 dark:text-gray-400">
                    Python 脚本库 ({scripts.length})
                  </span>
                  <button
                    onClick={loadScripts}
                    disabled={loadingScripts}
                    className="p-1 text-gray-400 hover:text-gray-600 rounded"
                    title="刷新"
                  >
                    <RefreshCw className={`w-3.5 h-3.5 ${loadingScripts ? 'animate-spin' : ''}`} />
                  </button>
                </div>

                {/* 脚本列表 */}
                <div className="flex-1 overflow-auto p-2 space-y-1">
                  {loadingScripts ? (
                    <div className="flex items-center justify-center py-8 text-gray-400">
                      <Loader2 className="w-5 h-5 animate-spin mr-2" />
                      加载中...
                    </div>
                  ) : scripts.length === 0 ? (
                    <div className="text-center py-8 text-gray-400 text-sm">
                      <FileText className="w-8 h-8 mx-auto mb-2 opacity-50" />
                      <p>暂无脚本</p>
                      <p className="text-xs mt-1">可放入项目 `.pm_center/scripts/`，也可使用全局脚本目录</p>
                    </div>
                  ) : (
                    scripts.map(script => (
                      <button
                        key={script.id}
                        onClick={() => handleSelectScript(script)}
                        className={`w-full text-left p-3 rounded-lg border transition-all ${
                          selectedScriptId === script.id
                            ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20'
                            : 'border-gray-200 dark:border-gray-700 hover:border-gray-300 dark:hover:border-gray-600'
                        }`}
                        >
                          <div className="flex items-start gap-2">
                            <span className="text-blue-500 font-bold text-xs">PY</span>
                            <div className="flex-1 min-w-0">
                              <div className="flex items-center gap-2">
                                <div className="font-medium text-sm text-gray-900 dark:text-gray-100 truncate">
                                  {script.name}
                                </div>
                                <span
                                  className={`shrink-0 rounded-full px-1.5 py-0.5 text-[10px] ${
                                    script.scope === 'project'
                                      ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300'
                                      : 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300'
                                  }`}
                                >
                                  {script.scope === 'project' ? '项目' : '全局'}
                                </span>
                              </div>
                              <div className="text-xs text-gray-500 dark:text-gray-400 truncate mt-0.5">
                                {script.description}
                              </div>
                            <div className="text-xs text-gray-400 mt-1">{script.filename}</div>
                          </div>
                        </div>
                      </button>
                    ))
                  )}
                </div>
              </>
            ) : (
              /* 自定义模式 */
              <div className="flex-1 overflow-auto p-3">
                <div className="text-sm text-gray-600 dark:text-gray-400">
                  <p className="mb-2">自定义 Python 脚本</p>
                  <p className="text-xs text-gray-500">
                    使用 print(f&quot;进度 /***50*/&quot;, flush=True) 报告进度
                  </p>
                </div>
              </div>
            )}
          </div>

          {/* 右侧：表单 */}
          <form onSubmit={handleSubmit} className="flex-1 flex flex-col overflow-hidden">
            <div className="flex-1 overflow-auto p-5 space-y-4">
              {/* 任务名称 */}
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">任务名称 *</label>
                  <input
                    type="text"
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="例如：Blender渲染"
                    className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100"
                    required
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">子名称</label>
                  <input
                    type="text"
                    value={subName}
                    onChange={(e) => setSubName(e.target.value)}
                    placeholder="例如：场景_001.blend"
                    className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100"
                  />
                </div>
              </div>

              {/* 优先级和超时 */}
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">优先级</label>
                  <select
                    value={priority}
                    onChange={(e) => setPriority(e.target.value as TaskPriority)}
                    className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100"
                  >
                    <option value="high">高</option>
                    <option value="medium">中</option>
                    <option value="low">低</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">超时(分钟, 0=无限制)</label>
                  <input
                    type="number"
                    value={timeout}
                    onChange={(e) => setTimeout(parseInt(e.target.value) || 0)}
                    min={0}
                    className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100"
                  />
                </div>
              </div>

              {/* 脚本代码 */}
              <div className="flex-1 flex flex-col min-h-[200px]">
                <div className="flex items-center justify-between mb-1">
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
                    脚本代码 * {mode === 'select' && selectedScriptId && '(来自脚本库)'}
                  </label>
                  {mode === 'custom' && (
                    <button
                      type="button"
                      onClick={loadExample}
                      className="text-xs text-blue-600 hover:text-blue-700"
                    >
                      加载示例
                    </button>
                  )}
                </div>
                <textarea
                  value={code}
                  onChange={(e) => setCode(e.target.value)}
                  className="flex-1 min-h-[200px] w-full px-3 py-2 font-mono text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-gray-50 dark:bg-gray-950 text-gray-900 dark:text-gray-100 resize-none"
                  placeholder={`输入脚本代码...\n使用 /***数字*/ 格式报告进度，如 /***50*/ 表示50%`}
                  required
                />
              </div>

              {/* 提示 */}
              <div className="p-3 bg-blue-50 dark:bg-blue-900/20 rounded-lg text-sm text-blue-700 dark:text-blue-300">
                <p className="font-medium mb-1">💡 进度报告提示</p>
                <p>在脚本输出中包含 <code>/***数字*/</code> 格式即可自动更新进度条。</p>
                <p className="text-xs mt-1 opacity-80">例如：print(&quot;进度 /***80*/&quot;) 会将进度设为80%</p>
              </div>
            </div>

            {/* 底部按钮 */}
            <div className="flex justify-end gap-2 px-5 py-3 border-t border-gray-200 dark:border-gray-700">
              <button
                type="button"
                onClick={onClose}
                className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg"
              >
                取消
              </button>
              <button
                onClick={handleSubmit}
                disabled={!name || !code}
                className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-700 disabled:bg-gray-300 text-white rounded-lg"
              >
                添加任务
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}
