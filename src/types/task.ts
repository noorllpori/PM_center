import type { PluginActionContext, PluginInteractionResponse } from './plugin';

export type TaskStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
export type TaskPriority = 'high' | 'medium' | 'low';
export type ScriptType = 'python';

export interface PythonInlineTaskScript {
  kind: 'python-inline';
  code: string;
  type: ScriptType;
  interpreter?: string;
  workingDir?: string;
  envVars?: Record<string, string>;
}

export interface PluginActionTaskScript {
  kind: 'plugin-action';
  pluginKey: string;
  pluginId: string;
  pluginName: string;
  commandId: string;
  commandTitle: string;
  location: 'toolbar' | 'file-context';
  context: PluginActionContext;
  interactionResponses?: PluginInteractionResponse[];
}

export type TaskScript = PythonInlineTaskScript | PluginActionTaskScript;

export interface Task {
  id: string;
  projectPath: string;
  name: string;
  subName: string;
  script: TaskScript;
  status: TaskStatus;
  progress: number;
  createdAt: number;
  startedAt?: number;
  completedAt?: number;
  output: string[];
  fullLog: string;
  exitCode?: number;
  errorMessage?: string;
  priority: TaskPriority;
  maxRetries: number;
  currentRetry: number;
  timeout: number;
  dependencies: string[];
  schedule?: {
    enabled: boolean;
    cron: string;
    nextRun?: number;
  };
}

export interface TaskStats {
  total: number;
  pending: number;
  running: number;
  completed: number;
  failed: number;
  cancelled: number;
}

export interface TaskConfig {
  maxConcurrent: number;
  defaultTimeout: number;
  autoCleanDays: number;
  saveLogToFile: boolean;
}

export interface TaskTemplate {
  id: string;
  name: string;
  description: string;
  script: TaskScript;
  priority: TaskPriority;
  maxRetries: number;
  timeout: number;
}

export interface ProjectScript {
  id: string;
  name: string;
  description: string;
  filename: string;
  path: string;
  scriptType: ScriptType;
  scope: 'project' | 'global';
}
