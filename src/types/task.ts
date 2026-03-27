// 任务系统类型定义

export type TaskStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
export type TaskPriority = 'high' | 'medium' | 'low';
export type ScriptType = 'python';

export interface TaskScript {
  code: string;           // 原始代码
  type: ScriptType;       // 脚本类型
  interpreter?: string;   // 自定义解释器路径（可选）
  workingDir?: string;    // 工作目录
  envVars?: Record<string, string>; // 环境变量
}

export interface Task {
  id: string;
  projectPath: string;    // 所属项目路径
  name: string;           // 总名称（如"Blender渲染"）
  subName: string;        // 子名称（如"场景_001.blend"）
  script: TaskScript;
  
  // 状态
  status: TaskStatus;
  progress: number;       // 0-100
  
  // 时间
  createdAt: number;
  startedAt?: number;
  completedAt?: number;
  
  // 输出
  output: string[];       // 实时输出行（最近500行）
  fullLog: string;        // 完整日志
  exitCode?: number;
  errorMessage?: string;
  
  // 配置
  priority: TaskPriority;
  maxRetries: number;     // 最大重试次数
  currentRetry: number;   // 当前重试次数
  timeout: number;        // 超时时间（秒），0表示不限制
  
  // 依赖
  dependencies: string[]; // 依赖的任务ID
  
  // 定时任务
  schedule?: {
    enabled: boolean;
    cron: string;         // cron 表达式
    nextRun?: number;     // 下次运行时间
  };
}

// 任务统计
export interface TaskStats {
  total: number;
  pending: number;
  running: number;
  completed: number;
  failed: number;
  cancelled: number;
}

// 任务配置
export interface TaskConfig {
  maxConcurrent: number;  // 最大并发数
  defaultTimeout: number; // 默认超时
  autoCleanDays: number;  // 自动清理几天前的已完成任务
  saveLogToFile: boolean; // 是否保存日志到文件
}

// 任务模板
export interface TaskTemplate {
  id: string;
  name: string;
  description: string;
  script: TaskScript;
  priority: TaskPriority;
  maxRetries: number;
  timeout: number;
}

// 项目脚本（从 .pm_center/scripts 读取）
export interface ProjectScript {
  id: string;
  name: string;
  description: string;
  filename: string;
  path: string;
  scriptType: ScriptType;
}
