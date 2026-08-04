import type { LucideIcon } from 'lucide-react';
import { Clapperboard, Database, FlaskConical, MessageCircle } from 'lucide-react';
import type { ModuleContributions, PortDefinition } from '../types/platform';
import type {
  PlatformModuleDiagnostic,
  PlatformModuleState,
} from '../types/platformRuntime';

export const BUILTIN_MODULE_IDS = {
  automationRuntime: 'builtin.automation-runtime',
  lanCollaboration: 'builtin.lan-collaboration',
  projectResources: 'builtin.project-resources',
  renderCenter: 'builtin.render-center',
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
  cacheManager: contribution('builtin.project-resources.cache-tool', 'tools', BUILTIN_MODULE_IDS.projectResources),
  lanMain: contribution('builtin.lan-collaboration.main-tool', 'tools', BUILTIN_MODULE_IDS.lanCollaboration),
  lanProject: contribution('builtin.lan-collaboration.project-tool', 'tools', BUILTIN_MODULE_IDS.lanCollaboration),
  pythonEnvironments: contribution('builtin.automation-runtime.python-tool', 'tools', BUILTIN_MODULE_IDS.automationRuntime),
  taskCenter: contribution('builtin.automation-runtime.task-tool', 'tools', BUILTIN_MODULE_IDS.automationRuntime),
  settings: contribution('core.settings.tool', 'tools', null),
  mdtOverview: contribution('builtin.project-resources.mdt-tool', 'tools', BUILTIN_MODULE_IDS.projectResources),
  blenderFileParser: contribution('core.blender-file-parser.tool', 'tools', null),
  smartClipboard: contribution('builtin.smart-clipboard.tool', 'tools', BUILTIN_MODULE_IDS.smartClipboard),
  diagnosticSample: contribution(
    'diagnostic.contribution-sample.tool',
    'tools',
    DIAGNOSTIC_CONTRIBUTION_MODULE_ID,
  ),
} as const;

export type ContributionDataSourceScope = 'global' | 'project' | 'profile' | 'surface';
export type ContributionDataValueType = 'object' | 'list' | 'string' | 'number' | 'boolean';

export interface DataSourceContributionDefinition extends ContributionDefinition {
  title: string;
  description: string;
  scope: ContributionDataSourceScope;
  valueType: ContributionDataValueType;
}

export const DATA_SOURCE_CONTRIBUTIONS = {
  diagnosticRegistrySummary: {
    ...contribution(
      'diagnostic.contribution-sample.registry-data-source',
      'dataSources',
      DIAGNOSTIC_CONTRIBUTION_MODULE_ID,
    ),
    title: '贡献注册表摘要',
    description: '读取当前有效贡献、冲突和模块状态的只读摘要。',
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
    description: '用于验证工作流节点目录、端口定义和模块动态撤下。',
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

export interface SettingsSectionContributionDefinition extends ContributionDefinition {
  scopes: readonly ('global' | 'project')[];
  title: string;
}

export const SETTINGS_SECTION_CONTRIBUTIONS = {
  automationRuntime: {
    ...contribution(
      'builtin.automation-runtime.settings-section',
      'settingsSections',
      BUILTIN_MODULE_IDS.automationRuntime,
    ),
    scopes: ['global', 'project'],
    title: '任务脚本与插件',
  },
} as const satisfies Record<string, SettingsSectionContributionDefinition>;

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
  tabType: 'lan';
  title: string;
  surfaceId: string;
}

export const SHELL_TAB_CONTRIBUTIONS = {
  lan: {
    ...contribution('builtin.lan-collaboration.shell-tab', 'shellTabs', BUILTIN_MODULE_IDS.lanCollaboration),
    tabId: 'lan-collaboration',
    tabType: 'lan',
    title: '设备协作',
    surfaceId: 'builtin.lan-collaboration.main-surface',
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
    surfaceId: 'builtin.project-resources.cache-surface',
  },
  render: {
    ...contribution('builtin.render-center.workspace-tab', 'workspaceTabs', BUILTIN_MODULE_IDS.renderCenter),
    tabId: 'render-center',
    tabType: 'render',
    title: '渲染与批处理',
    icon: Clapperboard,
    iconClassName: 'text-orange-500',
    surfaceId: 'builtin.render-center.surface',
  },
  p2p: {
    ...contribution('builtin.lan-collaboration.project-workspace-tab', 'workspaceTabs', BUILTIN_MODULE_IDS.lanCollaboration),
    tabId: 'p2p-chat',
    tabType: 'p2p',
    title: '局域网项目功能',
    icon: MessageCircle,
    iconClassName: 'text-emerald-600',
    surfaceId: 'builtin.lan-collaboration.project-surface',
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
    surfaceId: 'diagnostic.contribution-sample.surface',
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
    return '模块贡献正在加载';
  }

  const conflict = snapshot.conflicts.find(
    (item) => item.kind === definition.kind && item.contributionId === definition.id,
  );
  if (conflict) {
    return `贡献 ID 被多个模块占用：${conflict.moduleIds.join('、')}`;
  }

  const module = snapshot.modulesById[definition.moduleId];
  if (!module) {
    return `提供该功能的模块未安装：${definition.moduleId}`;
  }
  if (snapshot.claims[definition.kind][definition.id] !== definition.moduleId) {
    return `模块清单未声明该功能：${definition.id}`;
  }
  return moduleStateReason(module.state);
}

function moduleStateReason(state: PlatformModuleState) {
  switch (state) {
    case 'disabled':
      return '提供该功能的模块已停用';
    case 'starting':
    case 'resolving':
      return '提供该功能的模块正在启动';
    case 'stopping':
      return '提供该功能的模块正在停止';
    case 'blocked':
      return '提供该功能的模块被依赖或冲突阻止';
    case 'restart-required':
      return '提供该功能的模块需要重启';
    case 'error':
      return '提供该功能的模块启动失败';
    default:
      return '提供该功能的模块当前不可用';
  }
}
