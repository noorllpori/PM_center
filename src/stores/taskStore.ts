import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Task, TaskStatus, TaskStats, TaskScript, TaskPriority } from '../types/task';
import { usePythonEnvStore } from './pythonEnvStore';

interface TaskState {
  // 任务列表
  tasks: Task[];
  activeTaskId: string | null; // 当前选中的任务
  
  // 统计
  stats: TaskStats;
  
  // 配置
  maxConcurrent: number;
  
  // 操作
  addTask: (task: Omit<Task, 'id' | 'status' | 'progress' | 'output' | 'fullLog' | 'createdAt' | 'currentRetry'>) => string;
  removeTask: (id: string) => void;
  cancelTask: (id: string) => Promise<void>;
  retryTask: (id: string) => Promise<void>;
  clearCompleted: () => void;
  clearAll: () => void;
  
  // 选择
  selectTask: (id: string | null) => void;
  
  // 内部
  updateTaskProgress: (id: string, progress: number) => void;
  updateTaskOutput: (id: string, line: string) => void;
  updateTaskStatus: (id: string, status: TaskStatus, exitCode?: number, error?: string) => void;
  processQueue: () => Promise<void>;
}

// 生成任务ID
function generateTaskId(): string {
  return `task_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
}

// 解析进度（从输出中提取 /***数字*/ 格式，如 /***50*/）
function parseProgress(line: string): number | null {
  // 匹配 /***N*/ 格式（3个星号包围数字）
  const match = line.match(/\/\*\*\*(\d{1,3})\*\//);
  if (match) {
    const progress = parseInt(match[1], 10);
    return Math.min(100, Math.max(0, progress));
  }
  return null;
}

// 计算统计
function calcStats(tasks: Task[]): TaskStats {
  return {
    total: tasks.length,
    pending: tasks.filter(t => t.status === 'pending').length,
    running: tasks.filter(t => t.status === 'running').length,
    completed: tasks.filter(t => t.status === 'completed').length,
    failed: tasks.filter(t => t.status === 'failed').length,
    cancelled: tasks.filter(t => t.status === 'cancelled').length,
  };
}

export const useTaskStore = create<TaskState>((set, get) => ({
  tasks: [],
  activeTaskId: null,
  stats: { total: 0, pending: 0, running: 0, completed: 0, failed: 0, cancelled: 0 },
  maxConcurrent: 3,

  // 添加任务（需要传入 projectPath）
  addTask: (taskData) => {
    const id = generateTaskId();
    const newTask: Task = {
      ...taskData,
      id,
      status: 'pending',
      progress: 0,
      output: [],
      fullLog: '',
      createdAt: Date.now(),
      currentRetry: 0,
    };

    set(state => {
      const tasks = [newTask, ...state.tasks];
      return {
        tasks,
        stats: calcStats(tasks),
      };
    });

    // 触发队列处理
    get().processQueue();

    return id;
  },

  // 删除任务
  removeTask: (id) => {
    set(state => {
      const tasks = state.tasks.filter(t => t.id !== id);
      return {
        tasks,
        stats: calcStats(tasks),
        activeTaskId: state.activeTaskId === id ? null : state.activeTaskId,
      };
    });
  },

  // 取消任务
  cancelTask: async (id) => {
    const task = get().tasks.find(t => t.id === id);
    if (!task || task.status === 'completed' || task.status === 'cancelled') {
      return;
    }

    try {
      await invoke('cancel_task', { taskId: id });
      get().updateTaskStatus(id, 'cancelled');
    } catch (error) {
      console.error('Failed to cancel task:', error);
    }
  },

  // 重试任务
  retryTask: async (id) => {
    const task = get().tasks.find(t => t.id === id);
    if (!task || (task.status !== 'failed' && task.status !== 'cancelled')) {
      return;
    }

    set(state => {
      const tasks = state.tasks.map(t =>
        t.id === id
          ? { ...t, status: 'pending' as TaskStatus, progress: 0, currentRetry: t.currentRetry + 1, errorMessage: undefined }
          : t
      );
      return { tasks, stats: calcStats(tasks) };
    });

    get().processQueue();
  },

  // 清理已完成的任务
  clearCompleted: () => {
    set(state => {
      const tasks = state.tasks.filter(t => t.status !== 'completed');
      return {
        tasks,
        stats: calcStats(tasks),
      };
    });
  },

  // 清理所有任务
  clearAll: () => {
    // 先取消运行中的任务
    const runningTasks = get().tasks.filter(t => t.status === 'running');
    runningTasks.forEach(t => get().cancelTask(t.id));
    
    set({
      tasks: [],
      activeTaskId: null,
      stats: { total: 0, pending: 0, running: 0, completed: 0, failed: 0, cancelled: 0 },
    });
  },

  // 选择任务
  selectTask: (id) => {
    set({ activeTaskId: id });
  },

  // 更新进度
  updateTaskProgress: (id, progress) => {
    set(state => ({
      tasks: state.tasks.map(t =>
        t.id === id ? { ...t, progress } : t
      ),
    }));
  },

  // 更新输出
  updateTaskOutput: (id, line) => {
    const progress = parseProgress(line);
    
    set(state => ({
        tasks: state.tasks.map(t => {
          if (t.id !== id) return t;
          
          const newOutput = [...t.output, line].slice(-500); // 保留最近500行
          return {
            ...t,
            output: newOutput,
            fullLog: t.fullLog + line + '\n',
            ...(progress !== null ? { progress } : {}),
          };
        }),
    }));
  },

  // 更新状态
  updateTaskStatus: (id, status, exitCode?, error?) => {
    set(state => {
      const tasks = state.tasks.map(t =>
        t.id === id
          ? {
              ...t,
              status,
              ...(status === 'running' ? { startedAt: Date.now() } : {}),
              ...(status === 'completed' || status === 'failed' || status === 'cancelled'
                ? { completedAt: Date.now() }
                : {}),
              ...(exitCode !== undefined ? { exitCode } : {}),
              ...(error ? { errorMessage: error } : {}),
            }
          : t
      );
      return {
        tasks,
        stats: calcStats(tasks),
      };
    });

    // 如果有任务完成，继续处理队列
    if (status === 'completed' || status === 'failed' || status === 'cancelled') {
      setTimeout(() => get().processQueue(), 100);
    }
  },

  // 处理任务队列
  processQueue: async () => {
    const { tasks, maxConcurrent } = get();
    
    const runningCount = tasks.filter(t => t.status === 'running').length;
    const pendingTasks = tasks
      .filter(t => t.status === 'pending')
      .sort((a, b) => {
        // 按优先级排序：high > medium > low
        const priorityOrder = { high: 0, medium: 1, low: 2 };
        return priorityOrder[a.priority] - priorityOrder[b.priority];
      });

    const canStart = Math.min(maxConcurrent - runningCount, pendingTasks.length);
    
    // 获取选中的 Python 环境
    const pythonEnvStore = usePythonEnvStore.getState();
    const selectedEnv = pythonEnvStore.envs.find(e => e.id === pythonEnvStore.selectedEnvId);
    const pythonPath = selectedEnv?.path;
    
    for (let i = 0; i < canStart; i++) {
      const task = pendingTasks[i];
      
      // 检查依赖是否完成
      const depsCompleted = task.dependencies.every(depId => {
        const dep = tasks.find(t => t.id === depId);
        return dep?.status === 'completed';
      });
      
      if (!depsCompleted) continue;

      get().updateTaskStatus(task.id, 'running');

      try {
        await invoke('run_task', {
          taskId: task.id,
          script: task.script,
          timeoutSeconds: task.timeout,
          pythonPath,
        });
      } catch (error) {
        console.error('Failed to start task:', error);
        get().updateTaskStatus(task.id, 'failed', undefined, String(error));
      }
    }
  },
}));

// 防止重复初始化
let isInitialized = false;

// 监听后端事件
export function initTaskEventListeners() {
  if (isInitialized) {
    console.log('[TaskStore] 事件监听已初始化，跳过');
    return;
  }
  isInitialized = true;
  console.log('[TaskStore] 初始化事件监听...');
  
  // 任务输出
  listen('task-output', (event: { payload: { taskId: string; line: string } }) => {
    const { taskId, line } = event.payload;
    console.log(`[TaskStore] 收到输出: ${line.substring(0, 50)}`);
    useTaskStore.getState().updateTaskOutput(taskId, line);
  });

  // 任务完成
  listen('task-completed', (event: { payload: { taskId: string; exitCode: number } }) => {
    const { taskId, exitCode } = event.payload;
    console.log(`[TaskStore] 任务完成: ${taskId}, exitCode: ${exitCode}`);
    const status = exitCode === 0 ? 'completed' : 'failed';
    useTaskStore.getState().updateTaskStatus(taskId, status, exitCode);
  });

  // 任务错误
  listen('task-error', (event: { payload: { taskId: string; error: string } }) => {
    const { taskId, error } = event.payload;
    console.log(`[TaskStore] 任务错误: ${taskId}, error: ${error}`);
    useTaskStore.getState().updateTaskStatus(taskId, 'failed', undefined, error);
  });
  
  console.log('[TaskStore] 事件监听初始化完成');
}
