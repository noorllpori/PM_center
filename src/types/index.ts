// 文件信息
export interface FileInfo {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: string | null;
  created: string | null;
  extension: string | null;
  thumbnail: string | null;
}

// 树节点
export interface TreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  children: TreeNode[];
}

// 标签
export interface Tag {
  id: string;
  name: string;
  color: string;
}

// 文件标签
export interface FileTag {
  file_path: string;
  tag_id: string;
}

// 文件元数据
export interface FileMetadata {
  file_path: string;
  status: string | null;
  notes: string | null;
  custom_data: Record<string, unknown> | null;
}

// 项目信息
export interface ProjectInfo {
  name: string;
  path: string;
  root_path: string;
}

// 列配置
export interface ColumnConfig {
  key: string;
  title: string;
  width: number;
  visible: boolean;
  sortable: boolean;
  align?: 'left' | 'center' | 'right';
}

// 显示规则
export interface DisplayRule {
  id: string;
  name: string;
  condition: {
    field: 'name' | 'extension' | 'path' | 'tag' | 'status';
    operator: 'contains' | 'startsWith' | 'endsWith' | 'equals' | 'regex';
    value: string;
  };
  action: {
    type: 'highlight' | 'badge' | 'textColor' | 'icon';
    color?: string;
    icon?: string;
    label?: string;
  };
  enabled: boolean;
}

// 视图模式
export type ViewMode = 'list' | 'grid' | 'thumbnail';

// Python 环境类型
export enum EnvType {
  System = 'System',
  Embedded = 'Embedded',
  Blender = 'Blender',
  Custom = 'Custom',
}

export interface PythonEnv {
  python_path: string;
  env_type: EnvType;
  version: string;
}

export interface ScriptResult {
  success: boolean;
  stdout: string;
  stderr: string;
  exit_code: number | null;
}

// 脚本定义
export interface Script {
  id: string;
  name: string;
  description: string;
  code: string;
  env_type: EnvType;
  category: string;
  is_builtin: boolean;
}

// 文件变更记录
export interface FileChange {
  id: number;
  project_path: string;
  file_path: string;
  change_type: string; // created, modified, deleted
  file_size: number | null;
  timestamp: number;
  depth: number;
}

// 任务系统类型
export * from './task';
