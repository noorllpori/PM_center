import type { LucideIcon } from 'lucide-react';
import { Clapperboard, Database, FlaskConical, MessageCircle } from 'lucide-react';
import type { ModuleContributions, PortDefinition } from '../types/platform';
import type {
  PlatformModuleDiagnostic,
  PlatformModuleState,
} from '../types/platformRuntime';

export const BUILTIN_MODULE_IDS = {
  automationRuntime: 'builtin.automation-runtime',
  desktopIntegration: 'builtin.desktop-integration',
  externalTools: 'builtin.external-tools',
  lanCollaboration: 'builtin.lan-collaboration',
  localWebConsole: 'builtin.local-web-console',
  mediaLibrary: 'builtin.media-library',
  projectManager: 'builtin.project-manager',
  projectResources: 'builtin.project-resources',
  renderCenter: 'builtin.render-center',
  scriptAutomation: 'builtin.script-automation',
  sessionRuntime: 'builtin.session-runtime',
  settingsCenter: 'builtin.settings-center',
  smartClipboard: 'builtin.smart-clipboard',
} as const;

export const DIAGNOSTIC_CONTRIBUTION_MODULE_ID = 'diagnostic.contribution-sample';

export const CONTRIBUTION_KINDS = [
  'shellTabs',
  'workspaceTabs',
  'tools',
  'surfaces',
  'widgets',
  'dataSources',
  'commands',
  'settingsSections',
  'contextCommands',
  'workflowNodes',
] as const;

export type ContributionKind = typeof CONTRIBUTION_KINDS[number];

export interface ContributionDefinition {
  id: string;
  kind: ContributionKind;
  moduleId: string | null;
}

export interface ContributionConflict {
  contributionId: string;
  kind: ContributionKind;
  moduleIds: string[];
}

export interface ContributionRegistrySnapshot {
  isLoaded: boolean;
  modulesById: Record<string, PlatformModuleDiagnostic>;
  claims: Record<ContributionKind, Record<string, string>>;
  conflicts: ContributionConflict[];
}

export const TOOL_CONTRIBUTIONS = {
  renderCenter: contribution('builtin.render-center.tool', 'tools', BUILTIN_MODULE_IDS.renderCenter),
  externalRenderStation: contribution(
    'builtin.render-center.external-station-tool',
    'tools',
    BUILTIN_MODULE_IDS.renderCenter,
  ),
  mediaLibrary: contribution('builtin.media-library.tool', 'tools', BUILTIN_MODULE_IDS.mediaLibrary),
  cacheManager: contribution('builtin.project-resources.cache-tool', 'tools', BUILTIN_MODULE_IDS.projectResources),
  lanMain: contribution('builtin.lan-collaboration.main-tool', 'tools', BUILTIN_MODULE_IDS.lanCollaboration),
  lanProject: contribution('builtin.lan-collaboration.project-tool', 'tools', BUILTIN_MODULE_IDS.lanCollaboration),
  pythonEnvironments: contribution('builtin.automation-runtime.python-tool', 'tools', BUILTIN_MODULE_IDS.automationRuntime),
  taskCenter: contribution('builtin.automation-runtime.task-tool', 'tools', BUILTIN_MODULE_IDS.automationRuntime),
  settings: contribution('core.settings.tool', 'tools', BUILTIN_MODULE_IDS.settingsCenter),
  mdtOverview: contribution('builtin.project-resources.mdt-tool', 'tools', BUILTIN_MODULE_IDS.projectResources),
  blenderFileParser: contribution(
    'core.blender-file-parser.tool',
    'tools',
    BUILTIN_MODULE_IDS.externalTools,
  ),
  smartClipboard: contribution('builtin.smart-clipboard.tool', 'tools', BUILTIN_MODULE_IDS.smartClipboard),
  localWebConsole: contribution(
    'builtin.local-web-console.tool',
    'tools',
    BUILTIN_MODULE_IDS.localWebConsole,
  ),
  scriptAutomation: contribution(
    'builtin.script-automation.studio-tool',
    'tools',
    BUILTIN_MODULE_IDS.scriptAutomation,
  ),
  diagnosticSample: contribution(
    'diagnostic.contribution-sample.tool',
    'tools',
    DIAGNOSTIC_CONTRIBUTION_MODULE_ID,
  ),
} as const;

export type SurfaceContributionHost = 'workspace' | 'shell' | 'dialog' | 'event' | 'native';

export interface SurfaceContributionDefinition extends ContributionDefinition {
  title: string;
  host: SurfaceContributionHost;
}

export const SURFACE_CONTRIBUTIONS = {
  projectHome: {
    ...contribution(
      'builtin.project-manager.home-surface',
      'surfaces',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    title: '项目主页',
    host: 'shell',
  },
  projectWorkspace: {
    ...contribution(
      'builtin.project-manager.project-workspace-surface',
      'surfaces',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    title: '项目工作区',
    host: 'shell',
  },
  automationPython: {
    ...contribution(
      'builtin.automation-runtime.python-surface',
      'surfaces',
      BUILTIN_MODULE_IDS.automationRuntime,
    ),
    title: 'Python 环境',
    host: 'dialog',
  },
  automationTasks: {
    ...contribution(
      'builtin.automation-runtime.task-surface',
      'surfaces',
      BUILTIN_MODULE_IDS.automationRuntime,
    ),
    title: '任务中心',
    host: 'dialog',
  },
  scriptAutomationStudio: {
    ...contribution(
      'builtin.script-automation.studio-surface',
      'surfaces',
      BUILTIN_MODULE_IDS.scriptAutomation,
    ),
    title: '脚本开发者工作台',
    host: 'dialog',
  },
  renderCenter: {
    ...contribution(
      'builtin.render-center.surface',
      'surfaces',
      BUILTIN_MODULE_IDS.renderCenter,
    ),
    title: '渲染与批处理',
    host: 'workspace',
  },
  externalRenderStation: {
    ...contribution(
      'builtin.render-center.external-station-surface',
      'surfaces',
      BUILTIN_MODULE_IDS.renderCenter,
    ),
    title: '外部 Blender 渲染器',
    host: 'shell',
  },
  mediaLibrary: {
    ...contribution(
      'builtin.media-library.surface',
      'surfaces',
      BUILTIN_MODULE_IDS.mediaLibrary,
    ),
    title: '媒体资料库',
    host: 'shell',
  },
  cacheManager: {
    ...contribution(
      'builtin.project-resources.cache-surface',
      'surfaces',
      BUILTIN_MODULE_IDS.projectResources,
    ),
    title: '缓存管理',
    host: 'workspace',
  },
  mdtOverview: {
    ...contribution(
      'builtin.project-resources.mdt-surface',
      'surfaces',
      BUILTIN_MODULE_IDS.projectResources,
    ),
    title: 'MDT 项目概览',
    host: 'event',
  },
  lanMain: {
    ...contribution(
      'builtin.lan-collaboration.main-surface',
      'surfaces',
      BUILTIN_MODULE_IDS.lanCollaboration,
    ),
    title: '设备协作',
    host: 'shell',
  },
  lanProject: {
    ...contribution(
      'builtin.lan-collaboration.project-surface',
      'surfaces',
      BUILTIN_MODULE_IDS.lanCollaboration,
    ),
    title: '局域网项目功能',
    host: 'workspace',
  },
  smartClipboard: {
    ...contribution(
      'builtin.smart-clipboard.native-surface',
      'surfaces',
      BUILTIN_MODULE_IDS.smartClipboard,
    ),
    title: '智能剪贴板',
    host: 'native',
  },
  diagnosticSample: {
    ...contribution(
      'diagnostic.contribution-sample.surface',
      'surfaces',
      DIAGNOSTIC_CONTRIBUTION_MODULE_ID,
    ),
    title: '贡献隔离样本',
    host: 'workspace',
  },
} as const satisfies Record<string, SurfaceContributionDefinition>;

export const SURFACE_CONTRIBUTION_BY_ID = new Map<string, SurfaceContributionDefinition>(
  Object.values(SURFACE_CONTRIBUTIONS).map((definition) => [definition.id, definition] as const),
);

export type ContributionDataSourceScope = 'global' | 'project' | 'profile' | 'surface';
export type ContributionDataValueType = 'object' | 'list' | 'string' | 'number' | 'boolean';

export interface DataSourceContributionDefinition extends ContributionDefinition {
  title: string;
  description: string;
  scope: ContributionDataSourceScope;
  valueType: ContributionDataValueType;
}

export const DATA_SOURCE_CONTRIBUTIONS = {
  projectDirectory: {
    ...contribution(
      'builtin.project-manager.project-directory-data-source',
      'dataSources',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    title: '项目目录状态',
    description: '读取项目根目录和被忽略项目数量。',
    scope: 'global',
    valueType: 'object',
  },
  projectQuickActions: {
    ...contribution(
      'builtin.project-manager.quick-actions-data-source',
      'dataSources',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    title: '项目快捷操作状态',
    description: '读取项目创建、导入和忽略列表入口所需状态。',
    scope: 'surface',
    valueType: 'object',
  },
  recentProjects: {
    ...contribution(
      'builtin.project-manager.recent-projects-data-source',
      'dataSources',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    title: '最近项目',
    description: '读取软件级最近打开项目记录。',
    scope: 'global',
    valueType: 'object',
  },
  projectCatalog: {
    ...contribution(
      'builtin.project-manager.project-catalog-data-source',
      'dataSources',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    title: '项目目录扫描结果',
    description: '读取项目根目录扫描状态和项目条目。',
    scope: 'surface',
    valueType: 'object',
  },
  diagnosticRegistrySummary: {
    ...contribution(
      'diagnostic.contribution-sample.registry-data-source',
      'dataSources',
      DIAGNOSTIC_CONTRIBUTION_MODULE_ID,
    ),
    title: '贡献注册表摘要',
    description: '读取当前有效贡献、冲突和组件状态的只读摘要。',
    scope: 'global',
    valueType: 'object',
  },
} as const satisfies Record<string, DataSourceContributionDefinition>;

export const DATA_SOURCE_CONTRIBUTION_BY_ID = new Map<string, DataSourceContributionDefinition>(
  Object.values(DATA_SOURCE_CONTRIBUTIONS).map((definition) => [definition.id, definition] as const),
);

export interface WidgetContributionDefinition extends ContributionDefinition {
  title: string;
  description: string;
  dataSourceId?: string;
  minColumns: number;
  minRows: number;
}

export const WIDGET_CONTRIBUTIONS = {
  projectDirectory: {
    ...contribution(
      'builtin.project-manager.project-directory-widget',
      'widgets',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    title: '项目目录',
    description: '选择或清除用于扫描多个项目的根目录。',
    dataSourceId: DATA_SOURCE_CONTRIBUTIONS.projectDirectory.id,
    minColumns: 4,
    minRows: 2,
  },
  projectQuickActions: {
    ...contribution(
      'builtin.project-manager.quick-actions-widget',
      'widgets',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    title: '快速操作',
    description: '创建、导入项目并管理忽略列表。',
    dataSourceId: DATA_SOURCE_CONTRIBUTIONS.projectQuickActions.id,
    minColumns: 4,
    minRows: 2,
  },
  recentProjects: {
    ...contribution(
      'builtin.project-manager.recent-projects-widget',
      'widgets',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    title: '最近打开',
    description: '显示最近打开的项目。',
    dataSourceId: DATA_SOURCE_CONTRIBUTIONS.recentProjects.id,
    minColumns: 8,
    minRows: 4,
  },
  projectCatalog: {
    ...contribution(
      'builtin.project-manager.project-catalog-widget',
      'widgets',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    title: '项目列表',
    description: '显示项目根目录中扫描到的项目。',
    dataSourceId: DATA_SOURCE_CONTRIBUTIONS.projectCatalog.id,
    minColumns: 8,
    minRows: 4,
  },
  diagnosticRegistrySummary: {
    ...contribution(
      'diagnostic.contribution-sample.registry-widget',
      'widgets',
      DIAGNOSTIC_CONTRIBUTION_MODULE_ID,
    ),
    title: '贡献注册表状态',
    description: '显示贡献目录、有效声明和冲突状态。',
    dataSourceId: DATA_SOURCE_CONTRIBUTIONS.diagnosticRegistrySummary.id,
    minColumns: 4,
    minRows: 2,
  },
} as const satisfies Record<string, WidgetContributionDefinition>;

export const WIDGET_CONTRIBUTION_BY_ID = new Map<string, WidgetContributionDefinition>(
  Object.values(WIDGET_CONTRIBUTIONS).map((definition) => [definition.id, definition] as const),
);

export type CommandContributionScope = 'global' | 'project' | 'surface';

export interface CommandContributionDefinition extends ContributionDefinition {
  title: string;
  description: string;
  scope: CommandContributionScope;
}

export const COMMAND_CONTRIBUTIONS = {
  selectProjectRoot: projectManagerCommand(
    'select-project-root',
    '选择项目根目录',
    '选择用于扫描多个项目的根目录。',
  ),
  clearProjectRoot: projectManagerCommand(
    'clear-project-root',
    '清除项目根目录',
    '停止从当前根目录展示项目列表。',
  ),
  createProject: projectManagerCommand(
    'create-project',
    '创建项目',
    '在当前项目根目录中创建并打开项目。',
  ),
  importProject: projectManagerCommand(
    'import-project',
    '导入项目',
    '从系统目录选择器导入并打开项目。',
  ),
  openProject: projectManagerCommand(
    'open-project',
    '打开项目',
    '通过项目路径执行统一打开预检。',
  ),
  ignoreProject: projectManagerCommand(
    'ignore-project',
    '忽略项目',
    '从根目录项目列表隐藏指定项目。',
  ),
  restoreIgnoredProject: projectManagerCommand(
    'restore-ignored-project',
    '恢复忽略项目',
    '让指定项目重新出现在根目录项目列表中。',
  ),
  showIgnoredProjects: projectManagerCommand(
    'show-ignored-projects',
    '查看忽略项目',
    '打开被忽略项目列表。',
  ),
  removeRecentProject: projectManagerCommand(
    'remove-recent-project',
    '移除最近项目',
    '从最近项目记录中移除指定项目。',
  ),
} as const satisfies Record<string, CommandContributionDefinition>;

export const COMMAND_CONTRIBUTION_BY_ID = new Map<string, CommandContributionDefinition>(
  Object.values(COMMAND_CONTRIBUTIONS).map((definition) => [definition.id, definition] as const),
);

export interface WorkflowNodeContributionDefinition extends ContributionDefinition {
  title: string;
  description: string;
  category: string;
  command: string;
  inputs: readonly PortDefinition[];
  outputs: readonly PortDefinition[];
  executable: boolean;
}

export const WORKFLOW_NODE_CONTRIBUTIONS = {
  diagnosticEcho: {
    ...contribution(
      'diagnostic.contribution-sample.echo-node',
      'workflowNodes',
      DIAGNOSTIC_CONTRIBUTION_MODULE_ID,
    ),
    title: '诊断回显',
    description: '用于验证工作流节点目录、端口定义和组件动态撤下。',
    category: '诊断',
    command: 'diagnostic.echo',
    inputs: [
      { name: 'value', type: 'string', required: true, description: '需要回显的文本' },
    ],
    outputs: [
      { name: 'value', type: 'string', required: true, description: '原样输出的文本' },
    ],
    executable: false,
  },
} as const satisfies Record<string, WorkflowNodeContributionDefinition>;

export const WORKFLOW_NODE_CONTRIBUTION_BY_ID = new Map<string, WorkflowNodeContributionDefinition>(
  Object.values(WORKFLOW_NODE_CONTRIBUTIONS).map((definition) => [definition.id, definition] as const),
);

export type SettingsScope = 'global' | 'project';

export type SettingsNavigationIconKey =
  | 'about'
  | 'automation'
  | 'components'
  | 'desktop'
  | 'exclusions'
  | 'history'
  | 'platform'
  | 'session'
  | 'sliders'
  | 'tools'
  | 'web-console';

export type SettingsSectionRendererId =
  | 'builtin.automation-runtime.global-settings'
  | 'builtin.automation-runtime.project-settings'
  | 'builtin.desktop-integration.settings'
  | 'builtin.external-tools.settings'
  | 'builtin.local-web-console.settings'
  | 'builtin.project-manager.history-settings'
  | 'builtin.project-resources.global-exclusions-settings'
  | 'builtin.project-resources.project-rules-settings'
  | 'builtin.session-runtime.settings'
  | 'builtin.settings-center.component-settings-global'
  | 'builtin.settings-center.component-settings-project'
  | 'core.settings.about-settings';

export type SettingsSectionAvailability = 'always' | 'module-running';

export interface SettingsSectionContributionDefinition extends ContributionDefinition {
  owner: string;
  scopes: readonly SettingsScope[];
  order: number;
  navigationId: string;
  title: string;
  iconKey: SettingsNavigationIconKey;
  availability: SettingsSectionAvailability;
  rendererId: SettingsSectionRendererId;
}

export const SETTINGS_SECTION_CONTRIBUTIONS = {
  session: settingsSection({
    id: 'builtin.session-runtime.settings-section',
    moduleId: BUILTIN_MODULE_IDS.sessionRuntime,
    owner: BUILTIN_MODULE_IDS.sessionRuntime,
    scope: 'global',
    order: 100,
    navigationId: 'session',
    title: '会话',
    iconKey: 'session',
    rendererId: 'builtin.session-runtime.settings',
  }),
  desktop: settingsSection({
    id: 'builtin.desktop-integration.settings-section',
    moduleId: BUILTIN_MODULE_IDS.desktopIntegration,
    owner: BUILTIN_MODULE_IDS.desktopIntegration,
    scope: 'global',
    order: 150,
    navigationId: 'desktop',
    title: '桌面集成',
    iconKey: 'desktop',
    rendererId: 'builtin.desktop-integration.settings',
  }),
  localWebConsole: settingsSection({
    id: 'builtin.local-web-console.settings-section',
    moduleId: BUILTIN_MODULE_IDS.localWebConsole,
    owner: BUILTIN_MODULE_IDS.localWebConsole,
    scope: 'global',
    order: 200,
    navigationId: 'web-console',
    title: '网页控制台',
    iconKey: 'web-console',
    rendererId: 'builtin.local-web-console.settings',
  }),
  globalExclusions: settingsSection({
    id: 'builtin.project-resources.global-exclusions-settings-section',
    moduleId: BUILTIN_MODULE_IDS.projectResources,
    owner: BUILTIN_MODULE_IDS.projectResources,
    scope: 'global',
    order: 300,
    navigationId: 'exclusions',
    title: '排除规则',
    iconKey: 'exclusions',
    rendererId: 'builtin.project-resources.global-exclusions-settings',
  }),
  automationRuntime: settingsSection({
    id: 'builtin.automation-runtime.settings-section',
    moduleId: BUILTIN_MODULE_IDS.automationRuntime,
    owner: BUILTIN_MODULE_IDS.automationRuntime,
    scope: 'global',
    order: 400,
    navigationId: 'automation',
    title: '脚本与插件',
    iconKey: 'automation',
    rendererId: 'builtin.automation-runtime.global-settings',
  }),
  tools: settingsSection({
    id: 'builtin.external-tools.settings-section',
    moduleId: BUILTIN_MODULE_IDS.externalTools,
    owner: BUILTIN_MODULE_IDS.externalTools,
    scope: 'global',
    order: 500,
    navigationId: 'tools',
    title: '工具与 Blender',
    iconKey: 'tools',
    rendererId: 'builtin.external-tools.settings',
  }),
  projectHistory: settingsSection({
    id: 'builtin.project-manager.history-settings-section',
    moduleId: BUILTIN_MODULE_IDS.projectManager,
    owner: BUILTIN_MODULE_IDS.projectManager,
    scope: 'global',
    order: 600,
    navigationId: 'history',
    title: '历史记录',
    iconKey: 'history',
    rendererId: 'builtin.project-manager.history-settings',
  }),
  componentSettingsGlobal: settingsSection({
    id: 'builtin.settings-center.component-settings-global-section',
    moduleId: BUILTIN_MODULE_IDS.settingsCenter,
    owner: BUILTIN_MODULE_IDS.settingsCenter,
    scope: 'global',
    order: 700,
    navigationId: 'components',
    title: '组件设置',
    iconKey: 'components',
    rendererId: 'builtin.settings-center.component-settings-global',
  }),
  componentSettingsProject: settingsSection({
    id: 'builtin.settings-center.component-settings-project-section',
    moduleId: BUILTIN_MODULE_IDS.settingsCenter,
    owner: BUILTIN_MODULE_IDS.settingsCenter,
    scope: 'project',
    order: 300,
    navigationId: 'project-components',
    title: '组件设置',
    iconKey: 'components',
    rendererId: 'builtin.settings-center.component-settings-project',
  }),
  about: settingsSection({
    id: 'core.settings.about-section',
    moduleId: BUILTIN_MODULE_IDS.settingsCenter,
    owner: BUILTIN_MODULE_IDS.settingsCenter,
    scope: 'global',
    order: 800,
    navigationId: 'about',
    title: '关于与退出',
    iconKey: 'about',
    rendererId: 'core.settings.about-settings',
  }),
  projectRules: settingsSection({
    id: 'builtin.project-resources.project-rules-settings-section',
    moduleId: BUILTIN_MODULE_IDS.projectResources,
    owner: BUILTIN_MODULE_IDS.projectResources,
    scope: 'project',
    order: 100,
    navigationId: 'project-rules',
    title: '项目规则',
    iconKey: 'exclusions',
    rendererId: 'builtin.project-resources.project-rules-settings',
  }),
  projectPlugins: settingsSection({
    id: 'builtin.automation-runtime.project-settings-section',
    moduleId: BUILTIN_MODULE_IDS.automationRuntime,
    owner: BUILTIN_MODULE_IDS.automationRuntime,
    scope: 'project',
    order: 200,
    navigationId: 'project-plugins',
    title: '项目插件',
    iconKey: 'automation',
    rendererId: 'builtin.automation-runtime.project-settings',
  }),
} as const satisfies Record<string, SettingsSectionContributionDefinition>;

export function getAvailableSettingsSectionContributions(
  snapshot: ContributionRegistrySnapshot,
  scope: SettingsScope,
) {
  return Object.values(SETTINGS_SECTION_CONTRIBUTIONS)
    .filter((definition) => definition.scopes.includes(scope))
    .filter((definition) => (
      definition.availability === 'always'
        || isContributionAvailable(snapshot, definition)
    ))
    .sort((left, right) => left.order - right.order || left.id.localeCompare(right.id));
}

export type ContextCommandTarget = 'file' | 'directoryBackground' | 'collection';

export interface ContextCommandContributionDefinition extends ContributionDefinition {
  targets: readonly ContextCommandTarget[];
  title: string;
}

export const CONTEXT_COMMAND_CONTRIBUTIONS = {
  legacyPluginActions: {
    ...contribution(
      'builtin.automation-runtime.plugin-context-commands',
      'contextCommands',
      BUILTIN_MODULE_IDS.automationRuntime,
    ),
    targets: ['file', 'directoryBackground'],
    title: '插件右键动作',
  },
  projectCollections: {
    ...contribution(
      'builtin.project-resources.collection-context-commands',
      'contextCommands',
      BUILTIN_MODULE_IDS.projectResources,
    ),
    targets: ['file', 'collection'],
    title: '项目集合命令',
  },
} as const satisfies Record<string, ContextCommandContributionDefinition>;

export interface ShellTabContributionDefinition extends ContributionDefinition {
  tabId: string;
  tabType: 'lan' | 'project' | 'external-render-station' | 'media-library';
  instanceMode: 'singleton' | 'per-project';
  title: string;
  surfaceId: string;
}

export const SHELL_TAB_CONTRIBUTIONS = {
  project: {
    ...contribution(
      'builtin.project-manager.project-shell-tab',
      'shellTabs',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    tabId: 'project-workspace',
    tabType: 'project',
    instanceMode: 'per-project',
    title: '项目工作区',
    surfaceId: SURFACE_CONTRIBUTIONS.projectWorkspace.id,
  },
  lan: {
    ...contribution('builtin.lan-collaboration.shell-tab', 'shellTabs', BUILTIN_MODULE_IDS.lanCollaboration),
    tabId: 'lan-collaboration',
    tabType: 'lan',
    instanceMode: 'singleton',
    title: '设备协作',
    surfaceId: SURFACE_CONTRIBUTIONS.lanMain.id,
  },
  externalRenderStation: {
    ...contribution(
      'builtin.render-center.external-station-shell-tab',
      'shellTabs',
      BUILTIN_MODULE_IDS.renderCenter,
    ),
    tabId: 'external-render-station',
    tabType: 'external-render-station',
    instanceMode: 'singleton',
    title: '外部 Blender 渲染器',
    surfaceId: SURFACE_CONTRIBUTIONS.externalRenderStation.id,
  },
  mediaLibrary: {
    ...contribution(
      'builtin.media-library.shell-tab',
      'shellTabs',
      BUILTIN_MODULE_IDS.mediaLibrary,
    ),
    tabId: 'media-library',
    tabType: 'media-library',
    instanceMode: 'singleton',
    title: '媒体资料库',
    surfaceId: SURFACE_CONTRIBUTIONS.mediaLibrary.id,
  },
} as const satisfies Record<string, ShellTabContributionDefinition>;

export const SHELL_TAB_CONTRIBUTION_BY_ID = new Map<string, ShellTabContributionDefinition>(
  Object.values(SHELL_TAB_CONTRIBUTIONS).map((definition) => [definition.id, definition] as const),
);

export interface WorkspaceTabContributionDefinition extends ContributionDefinition {
  tabId: string;
  tabType: 'cache' | 'render' | 'p2p' | 'contribution';
  title: string;
  icon: LucideIcon;
  iconClassName: string;
  surfaceId: string;
}

export const WORKSPACE_TAB_CONTRIBUTIONS = {
  cache: {
    ...contribution('builtin.project-resources.cache-workspace-tab', 'workspaceTabs', BUILTIN_MODULE_IDS.projectResources),
    tabId: 'cache-manager',
    tabType: 'cache',
    title: '缓存管理',
    icon: Database,
    iconClassName: 'text-cyan-600',
    surfaceId: SURFACE_CONTRIBUTIONS.cacheManager.id,
  },
  render: {
    ...contribution('builtin.render-center.workspace-tab', 'workspaceTabs', BUILTIN_MODULE_IDS.renderCenter),
    tabId: 'render-center',
    tabType: 'render',
    title: '渲染与批处理',
    icon: Clapperboard,
    iconClassName: 'text-orange-500',
    surfaceId: SURFACE_CONTRIBUTIONS.renderCenter.id,
  },
  p2p: {
    ...contribution('builtin.lan-collaboration.project-workspace-tab', 'workspaceTabs', BUILTIN_MODULE_IDS.lanCollaboration),
    tabId: 'p2p-chat',
    tabType: 'p2p',
    title: '局域网项目功能',
    icon: MessageCircle,
    iconClassName: 'text-emerald-600',
    surfaceId: SURFACE_CONTRIBUTIONS.lanProject.id,
  },
  diagnosticSample: {
    ...contribution(
      'diagnostic.contribution-sample.workspace-tab',
      'workspaceTabs',
      DIAGNOSTIC_CONTRIBUTION_MODULE_ID,
    ),
    tabId: 'diagnostic-contribution-sample',
    tabType: 'contribution',
    title: '贡献隔离样本',
    icon: FlaskConical,
    iconClassName: 'text-fuchsia-500',
    surfaceId: SURFACE_CONTRIBUTIONS.diagnosticSample.id,
  },
} as const satisfies Record<string, WorkspaceTabContributionDefinition>;

export const WORKSPACE_TAB_CONTRIBUTION_BY_ID = new Map<string, WorkspaceTabContributionDefinition>(
  Object.values(WORKSPACE_TAB_CONTRIBUTIONS).map((definition) => [definition.id, definition] as const),
);

export const WORKSPACE_TAB_CONTRIBUTION_BY_TYPE = new Map<string, WorkspaceTabContributionDefinition>(
  Object.values(WORKSPACE_TAB_CONTRIBUTIONS).map((definition) => [definition.tabType, definition] as const),
);

export const EMPTY_CONTRIBUTION_REGISTRY = createEmptyContributionRegistry();

function contribution(
  id: string,
  kind: ContributionKind,
  moduleId: string | null,
): ContributionDefinition {
  return { id, kind, moduleId };
}

function settingsSection({
  id,
  moduleId,
  owner,
  scope,
  order,
  navigationId,
  title,
  iconKey,
  rendererId,
}: {
  id: string;
  moduleId: string | null;
  owner: string;
  scope: SettingsScope;
  order: number;
  navigationId: string;
  title: string;
  iconKey: SettingsNavigationIconKey;
  rendererId: SettingsSectionRendererId;
}): SettingsSectionContributionDefinition {
  return {
    ...contribution(id, 'settingsSections', moduleId),
    owner,
    scopes: [scope],
    order,
    navigationId,
    title,
    iconKey,
    availability: moduleId ? 'module-running' : 'always',
    rendererId,
  };
}

function projectManagerCommand(
  suffix: string,
  title: string,
  description: string,
): CommandContributionDefinition {
  return {
    ...contribution(
      `builtin.project-manager.${suffix}-command`,
      'commands',
      BUILTIN_MODULE_IDS.projectManager,
    ),
    title,
    description,
    scope: 'surface',
  };
}

function createClaims(): Record<ContributionKind, Record<string, string>> {
  return Object.fromEntries(
    CONTRIBUTION_KINDS.map((kind) => [kind, {}]),
  ) as Record<ContributionKind, Record<string, string>>;
}

function createEmptyContributionRegistry(): ContributionRegistrySnapshot {
  return {
    isLoaded: false,
    modulesById: {},
    claims: createClaims(),
    conflicts: [],
  };
}

function contributionIds(
  contributes: ModuleContributions | undefined,
  kind: ContributionKind,
): string[] {
  const values = contributes?.[kind];
  return Array.isArray(values)
    ? values.filter((value): value is string => typeof value === 'string')
    : [];
}

export function buildContributionRegistry(
  modules: PlatformModuleDiagnostic[],
): ContributionRegistrySnapshot {
  const claims = createClaims();
  const modulesById = Object.fromEntries(
    modules.map((module) => [module.manifest.id, module]),
  );
  const conflictOwners = new Map<string, Set<string>>();

  modules.forEach((module) => {
    CONTRIBUTION_KINDS.forEach((kind) => {
      contributionIds(module.manifest.contributes, kind).forEach((contributionId) => {
        const key = `${kind}:${contributionId}`;
        const existingOwner = claims[kind][contributionId];
        if (!existingOwner) {
          claims[kind][contributionId] = module.manifest.id;
          return;
        }
        if (existingOwner === module.manifest.id) {
          return;
        }

        const owners = conflictOwners.get(key) ?? new Set([existingOwner]);
        owners.add(module.manifest.id);
        conflictOwners.set(key, owners);
        delete claims[kind][contributionId];
      });
    });
  });

  return {
    isLoaded: true,
    modulesById,
    claims,
    conflicts: Array.from(conflictOwners.entries()).map(([key, moduleIds]) => {
      const separator = key.indexOf(':');
      return {
        kind: key.slice(0, separator) as ContributionKind,
        contributionId: key.slice(separator + 1),
        moduleIds: Array.from(moduleIds).sort(),
      };
    }),
  };
}

export function isContributionAvailable(
  snapshot: ContributionRegistrySnapshot,
  definition: ContributionDefinition,
) {
  if (!definition.moduleId) {
    return true;
  }
  if (!snapshot.isLoaded) {
    return false;
  }

  const module = snapshot.modulesById[definition.moduleId];
  return module?.state === 'running'
    && snapshot.claims[definition.kind][definition.id] === definition.moduleId;
}

export function isShellTabContributionAvailable(
  snapshot: ContributionRegistrySnapshot,
  definition: ShellTabContributionDefinition,
) {
  return getShellTabContributionUnavailableReason(snapshot, definition) === null;
}

export function getShellTabContributionUnavailableReason(
  snapshot: ContributionRegistrySnapshot,
  definition: ShellTabContributionDefinition,
) {
  return getContributionUnavailableReason(snapshot, definition)
    ?? getContributionUnavailableReason(snapshot, {
      id: definition.surfaceId,
      kind: 'surfaces',
      moduleId: definition.moduleId,
    });
}

export function isWorkspaceTabContributionAvailable(
  snapshot: ContributionRegistrySnapshot,
  definition: WorkspaceTabContributionDefinition,
) {
  return getWorkspaceTabContributionUnavailableReason(snapshot, definition) === null;
}

export function getWorkspaceTabContributionUnavailableReason(
  snapshot: ContributionRegistrySnapshot,
  definition: WorkspaceTabContributionDefinition,
) {
  return getContributionUnavailableReason(snapshot, definition)
    ?? getContributionUnavailableReason(snapshot, {
      id: definition.surfaceId,
      kind: 'surfaces',
      moduleId: definition.moduleId,
    });
}

export function getContributionUnavailableReason(
  snapshot: ContributionRegistrySnapshot,
  definition: ContributionDefinition,
) {
  if (isContributionAvailable(snapshot, definition)) {
    return null;
  }
  if (!definition.moduleId) {
    return null;
  }
  if (!snapshot.isLoaded) {
    return '组件贡献正在加载';
  }

  const conflict = snapshot.conflicts.find(
    (item) => item.kind === definition.kind && item.contributionId === definition.id,
  );
  if (conflict) {
    return `贡献 ID 被多个组件占用：${conflict.moduleIds.join('、')}`;
  }

  const module = snapshot.modulesById[definition.moduleId];
  if (!module) {
    return `提供该功能的组件未安装：${definition.moduleId}`;
  }
  if (snapshot.claims[definition.kind][definition.id] !== definition.moduleId) {
    return `组件清单未声明该功能：${definition.id}`;
  }
  return moduleStateReason(module.state);
}

function moduleStateReason(state: PlatformModuleState) {
  switch (state) {
    case 'disabled':
      return '提供该功能的组件已停用';
    case 'starting':
    case 'resolving':
      return '提供该功能的组件正在启动';
    case 'stopping':
      return '提供该功能的组件正在停止';
    case 'blocked':
      return '提供该功能的组件被依赖或冲突阻止';
    case 'restart-required':
      return '提供该功能的组件需要重启';
    case 'error':
      return '提供该功能的组件启动失败';
    default:
      return '提供该功能的组件当前不可用';
  }
}
