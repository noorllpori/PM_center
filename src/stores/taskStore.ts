import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { load } from '@tauri-apps/plugin-store';
import type { Task, TaskStatus, TaskStats, TaskScript, TaskPriority } from '../types/task';
import type { PluginControlMessage } from '../types/plugin';
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

interface PersistedTaskState {
  version: 1;
  tasks: Task[];
  activeTaskId: string | null;
  maxConcurrent: number;
}

const TASK_STORE_FILE = 'tasks.json';
const TASK_STORE_KEY = 'taskState';
const DEFAULT_MAX_CONCURRENT = 3;
const TASK_OUTPUT_LIMIT = 500;
const RECOVERED_TASK_MESSAGE = '[恢复] 应用重启后恢复，原任务已中断';
const EMPTY_TASK_STATS: TaskStats = {
  total: 0,
  pending: 0,
  running: 0,
  completed: 0,
  failed: 0,
  cancelled: 0,
};

let taskStorePromise: Promise<Awaited<ReturnType<typeof load>>> | null = null;
let taskStateLoadPromise: Promise<void> | null = null;
let persistTimer: ReturnType<typeof setTimeout> | null = null;
let persistQueue: Promise<void> = Promise.resolve();
let isHydratingTaskState = false;
let hasLoadedTaskState = false;

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

function getTaskPersistenceStore() {
  if (!taskStorePromise) {
    taskStorePromise = load(TASK_STORE_FILE);
  }
  return taskStorePromise;
}

function normalizeTask(task: Task): Task {
  const script = task.script as TaskScript & { kind?: TaskScript['kind'] };
  const normalizedScript: TaskScript = !script.kind || script.kind === 'python-inline'
    ? {
        kind: 'python-inline',
        code: script.kind === 'python-inline' ? script.code : (script as any).code,
        type: 'python',
        interpreter: 'interpreter' in script ? script.interpreter : undefined,
        workingDir: 'workingDir' in script ? script.workingDir ?? task.projectPath : task.projectPath,
        envVars: 'envVars' in script ? script.envVars : undefined,
      }
    : {
        ...script,
        interactionResponses: 'interactionResponses' in script ? script.interactionResponses ?? [] : [],
      };

  return {
    ...task,
    script: normalizedScript,
  };
}

function appendTaskLog(task: Task, line: string) {
  const output = [...task.output, line].slice(-TASK_OUTPUT_LIMIT);
  const prefix = task.fullLog && !task.fullLog.endsWith('\n') ? '\n' : '';
  return {
    output,
    fullLog: `${task.fullLog}${prefix}${line}\n`,
  };
}

function recoverInterruptedTask(task: Task, recoveredAt: number): Task {
  const normalizedTask = normalizeTask(task);
  const alreadyRecovered =
    normalizedTask.errorMessage === RECOVERED_TASK_MESSAGE
    || normalizedTask.output.some(line => line.includes(RECOVERED_TASK_MESSAGE))
    || normalizedTask.fullLog.includes(RECOVERED_TASK_MESSAGE);

  return {
    ...normalizedTask,
    ...(alreadyRecovered ? {} : appendTaskLog(normalizedTask, RECOVERED_TASK_MESSAGE)),
    status: 'cancelled',
    completedAt: normalizedTask.completedAt ?? recoveredAt,
    errorMessage: RECOVERED_TASK_MESSAGE,
  };
}

function buildPersistedTaskState(state: TaskState): PersistedTaskState {
  return {
    version: 1,
    tasks: state.tasks,
    activeTaskId: state.activeTaskId,
    maxConcurrent: state.maxConcurrent,
  };
}

async function persistTaskState() {
  const store = await getTaskPersistenceStore();
  await store.set(TASK_STORE_KEY, buildPersistedTaskState(useTaskStore.getState()));
  await store.save();
}

function scheduleTaskStatePersist() {
  if (isHydratingTaskState) {
    return;
  }

  if (persistTimer) {
    clearTimeout(persistTimer);
  }

  persistTimer = setTimeout(() => {
    persistTimer = null;
    persistQueue = persistQueue
      .then(() => persistTaskState())
      .catch(error => {
        console.error('Failed to persist task state:', error);
      });
  }, 200);
}

export async function loadTaskState() {
  if (hasLoadedTaskState) {
    return;
  }

  if (taskStateLoadPromise) {
    return taskStateLoadPromise;
  }

  taskStateLoadPromise = (async () => {
    try {
      const store = await getTaskPersistenceStore();
      const persisted = await store.get<PersistedTaskState>(TASK_STORE_KEY);

      if (!persisted) {
        hasLoadedTaskState = true;
        return;
      }

      const recoveredAt = Date.now();
      let recoveredInterrupted = false;

      const loadedTasks = persisted.tasks.map(task => {
        const normalizedTask = normalizeTask(task);
        if (normalizedTask.status === 'pending' || normalizedTask.status === 'running') {
          recoveredInterrupted = true;
          return recoverInterruptedTask(normalizedTask, recoveredAt);
        }
        return normalizedTask;
      });

      const currentState = useTaskStore.getState();
      const mergedTasks = [...currentState.tasks];
      const loadedTaskIds = new Set(mergedTasks.map(task => task.id));

      loadedTasks.forEach(task => {
        if (!loadedTaskIds.has(task.id)) {
          mergedTasks.push(task);
        }
      });

      mergedTasks.sort((a, b) => b.createdAt - a.createdAt);

      const nextActiveTaskId = currentState.activeTaskId && mergedTasks.some(task => task.id === currentState.activeTaskId)
        ? currentState.activeTaskId
        : (mergedTasks.some(task => task.id === persisted.activeTaskId) ? persisted.activeTaskId : null);

      isHydratingTaskState = true;
      useTaskStore.setState({
        tasks: mergedTasks,
        activeTaskId: nextActiveTaskId,
        maxConcurrent: persisted.maxConcurrent ?? DEFAULT_MAX_CONCURRENT,
        stats: calcStats(mergedTasks),
      });
      isHydratingTaskState = false;
      hasLoadedTaskState = true;

      if (recoveredInterrupted) {
        scheduleTaskStatePersist();
      }
    } catch (error) {
      console.error('Failed to load task state:', error);
    } finally {
      isHydratingTaskState = false;
      taskStateLoadPromise = null;
    }
  })();

  return taskStateLoadPromise;
}

export const useTaskStore = create<TaskState>((set, get) => ({
  tasks: [],
  activeTaskId: null,
  stats: EMPTY_TASK_STATS,
  maxConcurrent: DEFAULT_MAX_CONCURRENT,

  // 添加任务（需要传入 projectPath）
  addTask: (taskData) => {
    const id = generateTaskId();
    const normalizedScript = taskData.script.kind === 'python-inline'
      ? {
          ...taskData.script,
          workingDir: taskData.script.workingDir ?? taskData.projectPath,
        }
      : taskData.script;
    const newTask: Task = {
      ...taskData,
      script: normalizedScript,
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
        activeTaskId: tasks.some(t => t.id === state.activeTaskId) ? state.activeTaskId : null,
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
      stats: EMPTY_TASK_STATS,
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
          
          const newOutput = [...t.output, line].slice(-TASK_OUTPUT_LIMIT); // 保留最近500行
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

      void invoke('run_task', {
        taskId: task.id,
        script: task.script,
        timeoutSeconds: task.timeout,
        pythonPath: task.script.kind === 'python-inline' ? pythonPath : null,
      }).catch((error) => {
        console.error('Failed to start task:', error);

        const currentTask = useTaskStore.getState().tasks.find((item) => item.id === task.id);
        if (currentTask?.status === 'running') {
          useTaskStore.getState().updateTaskStatus(task.id, 'failed', undefined, String(error));
        }
      });
    }
  },
}));

useTaskStore.subscribe((state, previousState) => {
  if (
    state.tasks === previousState.tasks
    && state.activeTaskId === previousState.activeTaskId
    && state.maxConcurrent === previousState.maxConcurrent
  ) {
    return;
  }

  scheduleTaskStatePersist();
});

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

  listen('task-control', (event: { payload: { taskId: string; message: PluginControlMessage } }) => {
    const { taskId, message } = event.payload;
    if (message.type === 'progress' && typeof message.value === 'number') {
      useTaskStore.getState().updateTaskProgress(taskId, message.value);
    }

    if (message.type === 'error' && message.message) {
      useTaskStore.getState().updateTaskOutput(taskId, `[plugin-error] ${message.message}`);
    }

    if (message.type === 'result' && message.data !== undefined) {
      useTaskStore.getState().updateTaskOutput(taskId, `[plugin-result] ${JSON.stringify(message.data)}`);
    }

    if (message.type === 'confirm' && message.message) {
      useTaskStore.getState().updateTaskOutput(taskId, `[plugin-confirm] ${message.message}`);
    }
  });
  
  console.log('[TaskStore] 事件监听初始化完成');
}
