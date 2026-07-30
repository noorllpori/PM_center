import type { LucideIcon } from 'lucide-react';
import {
  Blocks,
  Clapperboard,
  Database,
  FileSearch,
  ListTodo,
  MessageCircle,
  NotebookTabs,
  Settings,
  Terminal,
} from 'lucide-react';

export type BuiltinToolId =
  | 'render-center'
  | 'cache-manager'
  | 'p2p-chat'
  | 'python-environments'
  | 'task-center'
  | 'settings'
  | 'mdt-overview'
  | 'blender-file-parser';

export type BuiltinToolCategory = 'project' | 'workflow' | 'system' | 'communication';

export interface BuiltinToolDefinition {
  id: BuiltinToolId;
  title: string;
  description: string;
  category: BuiltinToolCategory;
  icon: LucideIcon;
  requiresProject: boolean;
  pinnable: boolean;
  keywords: string[];
}

export const BUILTIN_TOOL_CATEGORY_LABELS: Record<BuiltinToolCategory, string> = {
  project: '项目管理',
  workflow: '工作流程',
  system: '环境与设置',
  communication: '协作',
};

export const BUILTIN_TOOL_CATEGORY_ORDER: BuiltinToolCategory[] = [
  'project',
  'workflow',
  'communication',
  'system',
];

export const BUILTIN_TOOLS: BuiltinToolDefinition[] = [
  {
    id: 'render-center',
    title: '渲染与批处理',
    description: '管理 Blender 渲染批次、队列、帧结果与视频打包。',
    category: 'workflow',
    icon: Clapperboard,
    requiresProject: true,
    pinnable: true,
    keywords: ['blender', 'render', '批渲染', '队列', '视频'],
  },
  {
    id: 'cache-manager',
    title: '缓存管理',
    description: '检查、清理和重建当前项目的 .pm_center 缓存。',
    category: 'project',
    icon: Database,
    requiresProject: true,
    pinnable: true,
    keywords: ['cache', 'pm_center', '缩略图', '目录树'],
  },
  {
    id: 'p2p-chat',
    title: '局域网消息',
    description: '发现局域网设备并发送项目协作消息。',
    category: 'communication',
    icon: MessageCircle,
    requiresProject: false,
    pinnable: true,
    keywords: ['p2p', 'lan', '聊天', '设备', '协作'],
  },
  {
    id: 'python-environments',
    title: 'Python 环境',
    description: '检测、创建和管理 PMC 使用的 Python 环境及依赖。',
    category: 'system',
    icon: Terminal,
    requiresProject: false,
    pinnable: true,
    keywords: ['python', 'venv', '环境', '依赖'],
  },
  {
    id: 'task-center',
    title: '任务中心',
    description: '查看脚本、插件、文件操作和渲染任务的运行状态。',
    category: 'workflow',
    icon: ListTodo,
    requiresProject: false,
    pinnable: true,
    keywords: ['task', '任务', '进度', '日志'],
  },
  {
    id: 'settings',
    title: '设置中心',
    description: '管理全局工具、Blender 版本、插件和当前项目设置。',
    category: 'system',
    icon: Settings,
    requiresProject: false,
    pinnable: true,
    keywords: ['setting', '配置', 'blender', 'ffmpeg', '插件'],
  },
  {
    id: 'mdt-overview',
    title: 'MDT 项目概览',
    description: '汇总当前项目的 MDT 任务、日志、引用文件和媒体。',
    category: 'project',
    icon: NotebookTabs,
    requiresProject: true,
    pinnable: true,
    keywords: ['mdt', 'markdown', '代办', '文档', '概览'],
  },
  {
    id: 'blender-file-parser',
    title: 'Blender 文件解析器',
    description: '读取 .blend 的场景、对象、材质、贴图和外部依赖。',
    category: 'workflow',
    icon: FileSearch,
    requiresProject: false,
    pinnable: true,
    keywords: ['blend', 'blender', '解析', '场景', '材质', '贴图'],
  },
];

export const DEFAULT_PINNED_BUILTIN_TOOL_IDS: BuiltinToolId[] = [
  'render-center',
  'cache-manager',
  'p2p-chat',
];

export const BUILTIN_TOOL_BY_ID = new Map(
  BUILTIN_TOOLS.map((tool) => [tool.id, tool] as const),
);

export function isBuiltinToolId(value: unknown): value is BuiltinToolId {
  return typeof value === 'string' && BUILTIN_TOOL_BY_ID.has(value as BuiltinToolId);
}

export type OpenBuiltinTool = (toolId: BuiltinToolId) => void;

export const BUILTIN_TOOLS_ICON = Blocks;
