import type { LucideIcon } from 'lucide-react';
import {
  Blocks,
  Clapperboard,
  ClipboardList,
  Database,
  FileSearch,
  FlaskConical,
  ListTodo,
  MessageCircle,
  MonitorCog,
  Network,
  NotebookTabs,
  Settings,
  Terminal,
} from 'lucide-react';
import {
  SHELL_TAB_CONTRIBUTIONS,
  TOOL_CONTRIBUTIONS,
  WORKSPACE_TAB_CONTRIBUTIONS,
  type ContributionDefinition,
} from './contributionRegistry';

export const OPEN_BUILTIN_TOOLS_CENTER_EVENT = 'pm-center:open-builtin-tools-center';

export type BuiltinToolId =
  | 'render-center'
  | 'cache-manager'
  | 'p2p-chat'
  | 'p2p-project'
  | 'python-environments'
  | 'task-center'
  | 'settings'
  | 'mdt-overview'
  | 'blender-file-parser'
  | 'smart-clipboard'
  | 'local-web-console'
  | 'contribution-diagnostics';

export type BuiltinToolCategory = 'project' | 'workflow' | 'system' | 'communication';

export type BuiltinToolDialogId =
  | 'python-environments'
  | 'task-center'
  | 'settings'
  | 'blender-file-parser';

export type BuiltinToolOpenTarget =
  | { type: 'workspaceTab'; contributionId: string }
  | { type: 'shellTab'; contributionId: string }
  | { type: 'dialog'; dialogId: BuiltinToolDialogId }
  | { type: 'event'; eventName: string }
  | { type: 'command'; command: string; errorTitle: string };

export interface BuiltinToolDefinition {
  id: BuiltinToolId;
  contribution: ContributionDefinition;
  title: string;
  description: string;
  help: string[];
  category: BuiltinToolCategory;
  icon: LucideIcon;
  requiresProject: boolean;
  pinnable: boolean;
  keywords: string[];
  openTarget: BuiltinToolOpenTarget;
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
    contribution: TOOL_CONTRIBUTIONS.renderCenter,
    title: '渲染与批处理',
    description: '管理 Blender 渲染批次、队列、帧结果与视频打包。',
    help: [
      '用于把当前项目中的 .blend 文件加入渲染批次，管理作业、帧队列、失败重试、暂停和继续。',
      '加入队列、调整顺序或重试帧不会自动开始渲染；需要明确点击“开始/继续队列”。',
      '支持常驻 Worker、逐帧兼容模式、任务级并发、性能统计和 ETA。完成帧及打包视频默认写入项目 renders/。',
    ],
    category: 'workflow',
    icon: Clapperboard,
    requiresProject: true,
    pinnable: true,
    keywords: ['blender', 'render', '批渲染', '队列', '视频'],
    openTarget: { type: 'workspaceTab', contributionId: WORKSPACE_TAB_CONTRIBUTIONS.render.id },
  },
  {
    id: 'cache-manager',
    contribution: TOOL_CONTRIBUTIONS.cacheManager,
    title: '缓存管理',
    description: '检查、清理和重建当前项目的 .pm_center 缓存。',
    help: [
      '查看当前项目的目录索引、缩略图和文件解析缓存占用，并执行快速检查或深度检查。',
      '清理只会处理可重新生成的缩略图和解析缓存；data.db、项目脚本、插件及渲染资料属于受保护数据。',
      '目录索引出现缺失或异常时使用“重建”，系统会原子替换索引，避免项目停留在无索引状态。',
    ],
    category: 'project',
    icon: Database,
    requiresProject: true,
    pinnable: true,
    keywords: ['cache', 'pm_center', '缩略图', '目录树'],
    openTarget: { type: 'workspaceTab', contributionId: WORKSPACE_TAB_CONTRIBUTIONS.cache.id },
  },
  {
    id: 'p2p-chat',
    contribution: TOOL_CONTRIBUTIONS.lanMain,
    title: '局域网主面板',
    description: '打开全局联系人、大厅和私聊主面板。',
    help: [
      '这是软件级局域网入口，用于查看在线设备、联系人、大厅消息和一对一私聊，不依赖当前项目。',
      '联系人、消息和头像保存在 PM Center 应用数据目录，不会写入项目的 .pm_center。',
      '文件发送需要对方确认；接收位置和自动接收规则可在局域网个人资料设置中管理。',
    ],
    category: 'communication',
    icon: MessageCircle,
    requiresProject: false,
    pinnable: true,
    keywords: ['p2p', 'lan', '聊天', '设备', '协作'],
    openTarget: { type: 'shellTab', contributionId: SHELL_TAB_CONTRIBUTIONS.lan.id },
  },
  {
    id: 'p2p-project',
    contribution: TOOL_CONTRIBUTIONS.lanProject,
    title: '局域网项目功能',
    description: '打开当前项目中的局域网功能预留标签。',
    help: [
      '在当前项目工作区中打开局域网功能标签，并随该项目的工作区会话一起恢复。',
      '这个入口用于后续项目协同、资源同步和渲染协作扩展；联系人、大厅和私聊仍由“局域网主面板”承载。',
      '当前没有打开项目时不可使用，且不会把全局联系人或聊天记录写入项目目录。',
    ],
    category: 'communication',
    icon: Network,
    requiresProject: true,
    pinnable: true,
    keywords: ['p2p', 'lan', '项目', '协同', '内置标签'],
    openTarget: { type: 'workspaceTab', contributionId: WORKSPACE_TAB_CONTRIBUTIONS.p2p.id },
  },
  {
    id: 'python-environments',
    contribution: TOOL_CONTRIBUTIONS.pythonEnvironments,
    title: 'Python 环境',
    description: '检测、创建和管理 PMC 使用的 Python 环境及依赖。',
    help: [
      '检测 PMC 内置 Python、系统 Python、虚拟环境和 Blender 自带 Python，并查看解释器版本与可用状态。',
      '可以创建或删除 venv、安装和卸载依赖包。项目脚本与插件会使用这里可用或已选择的环境。',
      '调整环境或依赖可能影响正在使用它的脚本和插件；执行删除或卸载前应先确认相关任务已停止。',
    ],
    category: 'system',
    icon: Terminal,
    requiresProject: false,
    pinnable: true,
    keywords: ['python', 'venv', '环境', '依赖'],
    openTarget: { type: 'dialog', dialogId: 'python-environments' },
  },
  {
    id: 'task-center',
    contribution: TOOL_CONTRIBUTIONS.taskCenter,
    title: '任务中心',
    description: '查看脚本、插件、文件操作和渲染任务的运行状态。',
    help: [
      '集中显示脚本、插件动作、文件操作和其他后台任务的状态、进度与运行日志。',
      '可以查看失败原因，并对支持的任务执行取消或重试；关闭任务中心不会终止仍在后台运行的任务。',
      '渲染批次的创建与帧管理仍在“渲染与批处理”中完成，任务中心主要用于观察执行过程。',
    ],
    category: 'workflow',
    icon: ListTodo,
    requiresProject: false,
    pinnable: true,
    keywords: ['task', '任务', '进度', '日志'],
    openTarget: { type: 'dialog', dialogId: 'task-center' },
  },
  {
    id: 'settings',
    contribution: TOOL_CONTRIBUTIONS.settings,
    title: '设置中心',
    description: '管理全局工具、Blender 版本、插件和当前项目设置。',
    help: [
      '管理 FFmpeg/FFprobe、Blender 版本、Python、开机启动、窗口行为和插件等软件级设置。',
      '打开项目后还可调整当前项目的排除规则及项目级插件设置；项目设置不会自动应用到其他项目。',
      '工具路径留空时 PM Center 会尝试从系统 PATH 自动检测，手动指定路径则优先使用指定版本。',
    ],
    category: 'system',
    icon: Settings,
    requiresProject: false,
    pinnable: true,
    keywords: ['setting', '配置', 'blender', 'ffmpeg', '插件'],
    openTarget: { type: 'dialog', dialogId: 'settings' },
  },
  {
    id: 'mdt-overview',
    contribution: TOOL_CONTRIBUTIONS.mdtOverview,
    title: 'MDT 项目概览',
    description: '汇总当前项目的 MDT 任务、日志、引用文件和媒体。',
    help: [
      '汇总当前项目中的 MDT 待办、时间记录、日志、图片或视频，以及任务引用的项目文件。',
      '适合按项目查看近期任务和关联资料；具体内容仍保存在对应 MDT 文档及项目文件中。',
      '该功能依赖当前项目，切换项目后会显示新项目自己的 MDT 数据。',
    ],
    category: 'project',
    icon: NotebookTabs,
    requiresProject: true,
    pinnable: true,
    keywords: ['mdt', 'markdown', '代办', '文档', '概览'],
    openTarget: { type: 'event', eventName: 'pm-center:open-mdt-overview' },
  },
  {
    id: 'blender-file-parser',
    contribution: TOOL_CONTRIBUTIONS.blenderFileParser,
    title: 'Blender 文件解析器',
    description: '读取 .blend 的场景、对象、材质、贴图和外部依赖。',
    help: [
      '选择一个 .blend 文件后，使用可用的 Blender 版本读取场景、对象、集合、材质、贴图和外部依赖。',
      '解析过程以只读方式检查文件，不会修改或保存 .blend；文件更新后可手动重新解析最新信息。',
      '可以用于排查缺失贴图、外链资源和场景结构，也可在未打开项目时分析项目外的 Blender 文件。',
    ],
    category: 'workflow',
    icon: FileSearch,
    requiresProject: false,
    pinnable: true,
    keywords: ['blend', 'blender', '解析', '场景', '材质', '贴图'],
    openTarget: { type: 'dialog', dialogId: 'blender-file-parser' },
  },
  {
    id: 'smart-clipboard',
    contribution: TOOL_CONTRIBUTIONS.smartClipboard,
    title: '智能剪贴板',
    description: '查看并恢复最近复制的文本、图像和文件。',
    help: [
      'PM Center 在后台运行时记录最近复制的文本、图像、文件和文件夹，最多保存 500 条并保留 30 天。',
      '使用 Ctrl+` 可在任意位置打开原生 Windows 历史窗口；输入文字搜索，上下键选择，Delete 删除，Esc 关闭。',
      'Enter 或双击会恢复内容并粘贴到之前的外部窗口；Ctrl+Enter 只恢复到系统剪贴板，不自动粘贴。',
    ],
    category: 'system',
    icon: ClipboardList,
    requiresProject: false,
    pinnable: true,
    keywords: ['clipboard', 'ditto', '剪贴板', '复制历史', 'Ctrl+`'],
    openTarget: {
      type: 'command',
      command: 'open_smart_clipboard',
      errorTitle: '智能剪贴板打开失败',
    },
  },
  {
    id: 'local-web-console',
    contribution: TOOL_CONTRIBUTIONS.localWebConsole,
    title: '本机网页控制台',
    description: '在真实浏览器中查看状态、修改部分设置并控制主窗口。',
    help: [
      '网页控制台只监听 127.0.0.1，默认关闭，需要在设置中明确启用后才能访问。',
      '浏览器入口使用持久访问令牌，只开放状态、部分常规设置、显示或隐藏主窗口、重启和退出等白名单操作。',
      '端口或网页权限修改后需要重启该模块；不会开放文件系统、Shell 或任意 Tauri 命令。',
    ],
    category: 'system',
    icon: MonitorCog,
    requiresProject: false,
    pinnable: true,
    keywords: ['web', 'browser', '网页', '浏览器', '控制台', 'localhost'],
    openTarget: {
      type: 'command',
      command: 'open_local_web_console',
      errorTitle: '网页控制台打开失败',
    },
  },
  {
    id: 'contribution-diagnostics',
    contribution: TOOL_CONTRIBUTIONS.diagnosticSample,
    title: '贡献隔离样本',
    description: '验证工具、页面、Widget、数据源和工作流节点的动态装配。',
    help: [
      '该入口只在“贡献隔离样本”诊断模块启用时出现，用于验证贡献注册表本身。',
      '页面通过通用工作区贡献打开，不在主应用打开逻辑中增加专用分支。',
      '停用诊断模块后，入口、Pin 和已打开标签会撤下；重新启用后数据目录重新可用。',
    ],
    category: 'system',
    icon: FlaskConical,
    requiresProject: true,
    pinnable: true,
    keywords: ['diagnostic', 'contribution', 'widget', 'datasource', 'workflow', '诊断'],
    openTarget: {
      type: 'workspaceTab',
      contributionId: WORKSPACE_TAB_CONTRIBUTIONS.diagnosticSample.id,
    },
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
